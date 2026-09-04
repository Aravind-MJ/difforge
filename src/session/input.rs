use crate::caret::{
    caret_in_wrapped, caret_on_line, char_len, delete_after, delete_before, insert_char, move_caret,
    move_caret_vert,
};
use crate::event::{CommitField, Effect, EffectKind, Focus, Key, MouseHit, Strip};

use super::Session;

impl Session {
    pub(super) fn handle_key(&mut self, key: Key) -> Vec<Effect> {
        if matches!(key, Key::Char { c: 'c', ctrl: true }) {
            return vec![self.effect(EffectKind::Quit)];
        }
        if matches!(key, Key::PageDown) {
            self.scroll_diff(self.page() as isize);
            return Vec::new();
        }
        if matches!(key, Key::PageUp) {
            self.scroll_diff(-(self.page() as isize));
            return Vec::new();
        }
        if self.overlay {
            return self.handle_overlay_key(key);
        }
        if self.search_open {
            return self.handle_search_key(key);
        }
        if let Some(effects) = self.handle_strip_key(key) {
            return effects;
        }
        if self.screen_inert() {
            if matches!(key, Key::Char { c: 'r', ctrl: false }) {
                return self.start_refresh();
            }
            return Vec::new();
        }
        self.handle_rest_key(key)
    }

    pub(super) fn handle_mouse(&mut self, hit: MouseHit) -> Vec<Effect> {
        if self.screen_inert() {
            return Vec::new();
        }
        match hit {
            MouseHit::Files { index } => {
                if self.search_open {
                    return Vec::new();
                }
                if index < self.rows.len() {
                    self.select(Some(index))
                } else {
                    Vec::new()
                }
            }
            MouseHit::Search { col } => {
                if self.search_open {
                    self.query_caret = caret_on_line(&self.query, col);
                }
                Vec::new()
            }
            MouseHit::Summary { col } => {
                if self.overlay {
                    self.field = CommitField::Summary;
                    self.summary_caret = caret_on_line(&self.draft.summary, col);
                }
                Vec::new()
            }
            MouseHit::Description { col, row } => {
                if self.overlay {
                    self.field = CommitField::Description;
                    self.description_caret =
                        caret_in_wrapped(&self.draft.description, col, row, self.field_width());
                }
                Vec::new()
            }
        }
    }

    fn screen_inert(&self) -> bool {
        matches!(
            self.strip,
            Some(
                Strip::NothingStagedConfirm
                    | Strip::NoFilesCommitError
                    | Strip::GitWriteError { .. }
            )
        )
    }

    fn handle_strip_key(&mut self, key: Key) -> Option<Vec<Effect>> {
        match &self.strip {
            Some(Strip::NothingStagedConfirm) => match key {
                Key::Char { c: 'y', ctrl: false } | Key::Enter { ctrl: false } => {
                    Some(self.git_add_all())
                }
                Key::Char { c: 'n', ctrl: false } | Key::Esc => {
                    self.strip = None;
                    Some(Vec::new())
                }
                Key::Char { c: 'r', ctrl: false } => Some(self.start_refresh()),
                _ => Some(Vec::new()),
            },
            Some(Strip::NoFilesCommitError) => match key {
                Key::Esc | Key::Enter { ctrl: false } => {
                    self.strip = None;
                    Some(Vec::new())
                }
                Key::Char { c: 'r', ctrl: false } => Some(self.start_refresh()),
                _ => Some(Vec::new()),
            },
            Some(Strip::GitWriteError { .. }) if !self.overlay => match key {
                Key::Esc | Key::Enter { ctrl: false } => {
                    self.strip = None;
                    Some(Vec::new())
                }
                Key::Char { c: 'r', ctrl: false } => Some(self.start_refresh()),
                _ => Some(Vec::new()),
            },
            _ => None,
        }
    }

    fn handle_rest_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Char { c: 'q', ctrl: false } => vec![self.effect(EffectKind::Quit)],
            Key::Char { c: 'j', ctrl: false } | Key::Down => self.move_or_scroll(1),
            Key::Char { c: 'k', ctrl: false } | Key::Up => self.move_or_scroll(-1),
            Key::Left | Key::Char { c: 'h', ctrl: false } => self.toggle_fold(),
            Key::Right | Key::Char { c: 'l', ctrl: false } => self.toggle_fold(),
            Key::Enter { ctrl: false } => self.toggle_fold(),
            Key::Tab => {
                self.focus = match self.focus {
                    Focus::Files => Focus::Diff,
                    Focus::Diff => Focus::Files,
                };
                Vec::new()
            }
            Key::Char { c: 'f', ctrl: false } => self.toggle_scope(),
            Key::Char { c: '`' | 't', ctrl: false } => self.toggle_shape(),
            Key::Char { c: '/', ctrl: false } => {
                self.search_open = true;
                self.query_caret = char_len(&self.query);
                Vec::new()
            }
            Key::Char { c: ' ', ctrl: false } => self.stage_selected(),
            Key::Char { c: 'c', ctrl: false } => self.open_commit(),
            Key::Char { c: 'r', ctrl: false } => self.start_refresh(),
            _ => Vec::new(),
        }
    }

    fn handle_search_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Enter { ctrl: false } => {
                self.search_open = false;
                Vec::new()
            }
            Key::Esc => {
                self.search_open = false;
                self.query.clear();
                self.query_caret = 0;
                self.rebuild_keep();
                self.request_dumps()
            }
            Key::Backspace => {
                delete_before(&mut self.query, &mut self.query_caret);
                self.rebuild_keep();
                self.request_dumps()
            }
            Key::Delete => {
                delete_after(&mut self.query, self.query_caret);
                self.rebuild_keep();
                self.request_dumps()
            }
            Key::Left => {
                move_caret(&mut self.query_caret, char_len(&self.query), -1);
                Vec::new()
            }
            Key::Right => {
                move_caret(&mut self.query_caret, char_len(&self.query), 1);
                Vec::new()
            }
            Key::Char { c, ctrl: false } => {
                insert_char(&mut self.query, &mut self.query_caret, c);
                self.rebuild_keep();
                self.request_dumps()
            }
            _ => Vec::new(),
        }
    }

    fn handle_overlay_key(&mut self, key: Key) -> Vec<Effect> {
        match key {
            Key::Esc => {
                self.overlay = false;
                self.strip = None;
                Vec::new()
            }
            Key::Tab => {
                self.field = match self.field {
                    CommitField::Summary => CommitField::Description,
                    CommitField::Description => CommitField::Summary,
                };
                Vec::new()
            }
            Key::Enter { ctrl: true } | Key::Char { c: 's' | 'S', ctrl: true } => self.try_commit(),
            Key::Enter { ctrl: false } => match self.field {
                CommitField::Summary => self.try_commit(),
                CommitField::Description => {
                    insert_char(&mut self.draft.description, &mut self.description_caret, '\n');
                    Vec::new()
                }
            },
            Key::Backspace => {
                match self.field {
                    CommitField::Summary => {
                        delete_before(&mut self.draft.summary, &mut self.summary_caret)
                    }
                    CommitField::Description => {
                        delete_before(&mut self.draft.description, &mut self.description_caret)
                    }
                }
                Vec::new()
            }
            Key::Delete => {
                match self.field {
                    CommitField::Summary => delete_after(&mut self.draft.summary, self.summary_caret),
                    CommitField::Description => {
                        delete_after(&mut self.draft.description, self.description_caret)
                    }
                }
                Vec::new()
            }
            Key::Left => {
                self.nudge_caret(-1);
                Vec::new()
            }
            Key::Right => {
                self.nudge_caret(1);
                Vec::new()
            }
            Key::Up => {
                if self.field == CommitField::Description {
                    self.description_caret = move_caret_vert(
                        &self.draft.description,
                        self.description_caret,
                        self.field_width(),
                        -1,
                    );
                }
                Vec::new()
            }
            Key::Down => {
                if self.field == CommitField::Description {
                    self.description_caret = move_caret_vert(
                        &self.draft.description,
                        self.description_caret,
                        self.field_width(),
                        1,
                    );
                }
                Vec::new()
            }
            Key::Char { c, ctrl: false } => {
                match self.field {
                    CommitField::Summary => {
                        insert_char(&mut self.draft.summary, &mut self.summary_caret, c)
                    }
                    CommitField::Description => {
                        insert_char(&mut self.draft.description, &mut self.description_caret, c)
                    }
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}
