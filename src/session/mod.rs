//! The Session is the product. `apply` is the only seam.

mod input;

use std::collections::HashSet;

use ratatui::text::Text;

use crate::caret::{char_len, description_line_count, move_caret, strip_height};
use crate::dump::{
    bytes_to_text, failure_text, pane_string, stack_texts, text_from_string, FailKind,
};
use crate::event::{
    ChildResult, CommitDraft, CommitField, Effect, EffectKind, Event, FilesMode, FilesRow, Focus,
    RowKind, Strip,
};
use crate::files::{
    folder_stack, has_staged, has_unstaged, is_in_head, is_untracked, parse_ls_files, parse_porcelain,
    row_color, visible_rows, PorcelainEntry,
};

const FILES_PANEL_COLS: u16 = 28;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Changed,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Flat,
    Tree,
}

enum DumpReq {
    Git { path: String, cached: bool },
    Difft { path: String },
    Literal(String),
}

struct PendingList {
    porcelain_id: u64,
    ls_id: u64,
    porcelain: Option<Vec<u8>>,
    ls: Option<Vec<u8>>,
}

struct PendingDumps {
    parts: Vec<DumpPart>,
}

enum DumpPart {
    Waiting { id: u64, fail: FailKind },
    Ready(Text<'static>),
}

enum WriteKind {
    Space,
    AddAll,
    Commit,
}

pub struct Session {
    cols: u16,
    term_rows: u16,
    focus: Focus,
    scope: Scope,
    shape: Shape,
    porcelain: Vec<PorcelainEntry>,
    ls_files: Vec<String>,
    collapsed: HashSet<String>,
    rows: Vec<FilesRow>,
    selected: Option<usize>,
    scroll: u16,
    dump_scroll_path: Option<String>,
    pane_body: String,
    pane_text: Text<'static>,
    pane_lines: u16,
    next_id: u64,
    pending_list: Option<PendingList>,
    pending_dumps: Option<PendingDumps>,
    strip: Option<Strip>,
    overlay: bool,
    search_open: bool,
    query: String,
    query_caret: usize,
    draft: CommitDraft,
    field: CommitField,
    summary_caret: usize,
    description_caret: usize,
    writing: Option<WriteKind>,
    write_id: Option<u64>,
    open_overlay_after_refresh: bool,
}

impl Session {
    pub fn boot(cols: u16, rows: u16) -> (Self, Vec<Effect>) {
        let mut session = Self {
            cols,
            term_rows: rows,
            focus: Focus::Files,
            scope: Scope::Changed,
            shape: Shape::Flat,
            porcelain: Vec::new(),
            ls_files: Vec::new(),
            collapsed: HashSet::new(),
            rows: Vec::new(),
            selected: None,
            scroll: 0,
            dump_scroll_path: None,
            pane_body: String::new(),
            pane_text: Text::default(),
            pane_lines: 0,
            next_id: 1,
            pending_list: None,
            pending_dumps: None,
            strip: None,
            overlay: false,
            search_open: false,
            query: String::new(),
            query_caret: 0,
            draft: CommitDraft::default(),
            field: CommitField::Summary,
            summary_caret: 0,
            description_caret: 0,
            writing: None,
            write_id: None,
            open_overlay_after_refresh: false,
        };
        let effects = session.start_refresh();
        (session, effects)
    }

    pub fn files_rows(&self) -> &[FilesRow] {
        &self.rows
    }

    pub fn selected_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn selected_path(&self) -> Option<&str> {
        self.selected
            .and_then(|i| self.rows.get(i))
            .map(|r| r.path.as_str())
    }

    pub fn focus(&self) -> Focus {
        self.focus
    }

    pub fn files_mode(&self) -> FilesMode {
        match (self.scope, self.shape) {
            (Scope::Changed, Shape::Flat) => FilesMode::ChangedFlat,
            (Scope::Changed, Shape::Tree) => FilesMode::ChangedTree,
            (Scope::All, _) => FilesMode::AllTree,
        }
    }

    pub fn diff_scroll(&self) -> u16 {
        self.scroll
    }

    pub fn pane_body(&self) -> &str {
        &self.pane_body
    }

    pub fn pane_text(&self) -> &Text<'static> {
        &self.pane_text
    }

    pub fn dumps_pending(&self) -> bool {
        self.pending_dumps.is_some()
    }

    pub fn strip(&self) -> Option<&Strip> {
        self.strip.as_ref()
    }

    pub fn search_query(&self) -> &str {
        &self.query
    }

    pub fn search_open(&self) -> bool {
        self.search_open
    }

    pub fn overlay_open(&self) -> bool {
        self.overlay
    }

    pub fn draft(&self) -> &CommitDraft {
        &self.draft
    }

    pub fn commit_field(&self) -> Option<CommitField> {
        self.overlay.then_some(self.field)
    }

    pub fn caret(&self) -> usize {
        if self.search_open {
            return self.query_caret;
        }
        if self.overlay {
            return match self.field {
                CommitField::Summary => self.summary_caret,
                CommitField::Description => self.description_caret,
            };
        }
        0
    }

    pub fn poll_paused(&self) -> bool {
        self.writing.is_some()
            || self.pending_list.is_some()
            || self.pending_dumps.is_some()
            || self.overlay
            || self.search_open
            || self.strip.is_some()
    }

    pub fn pane_width(&self) -> u16 {
        self.cols.saturating_sub(FILES_PANEL_COLS).max(1)
    }

    pub fn files_panel_cols(&self) -> u16 {
        FILES_PANEL_COLS.min(self.cols.saturating_sub(1).max(1))
    }

    pub fn overlay_height(&self) -> u16 {
        let form = if self.overlay {
            strip_height(description_line_count(&self.draft.description))
        } else {
            0
        };
        let extra = match &self.strip {
            Some(Strip::GitWriteError { text }) => {
                (text.split('\n').count() as u16).min(8).max(1)
            }
            Some(Strip::EmptySummaryRefusal) if self.overlay => 1,
            Some(_) if !self.overlay => 3,
            _ => 0,
        };
        if self.overlay {
            form.saturating_add(extra)
        } else {
            extra
        }
    }

    pub fn strip_text(&self) -> Option<String> {
        match &self.strip {
            Some(Strip::NothingStagedConfirm) => {
                Some("Nothing staged. Stage all changes and commit?".into())
            }
            Some(Strip::NoFilesCommitError) => Some("No files to commit.".into()),
            Some(Strip::EmptySummaryRefusal) => Some("Commit summary is required.".into()),
            Some(Strip::GitWriteError { text }) => Some(text.clone()),
            None => None,
        }
    }

    pub fn footer_text(&self) -> String {
        let n = self.rows.len();
        if self.search_open {
            return format!(" /{}_  enter keep  esc clear ", self.query);
        }
        let jk = match self.focus {
            Focus::Files => "j/k move",
            Focus::Diff => "j/k scroll",
        };
        let keys = format!(
            "{jk}  h/l fold  f all/changed  ` tree/flat  / search  tab focus  space stage  c commit  r refresh  q quit"
        );
        if !self.query.is_empty() {
            format!(" /{}  {n} rows  {keys} ", self.query)
        } else {
            format!(" {n} rows  {keys} ")
        }
    }
}

pub fn apply(mut session: Session, event: Event) -> (Session, Vec<Effect>) {
    let effects = session.handle(event);
    (session, effects)
}

impl Session {
    fn handle(&mut self, event: Event) -> Vec<Effect> {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Mouse(hit) => self.handle_mouse(hit),
            Event::Tick => {
                if self.poll_paused() {
                    Vec::new()
                } else {
                    self.start_refresh()
                }
            }
            Event::Resize { cols, rows } => {
                let old = self.pane_width();
                self.cols = cols;
                self.term_rows = rows;
                if self.pane_width() != old {
                    self.request_dumps()
                } else {
                    Vec::new()
                }
            }
            Event::Child(child) => self.handle_child(child),
        }
    }

    fn handle_child(&mut self, child: ChildResult) -> Vec<Effect> {
        if self.write_id == Some(child.id) {
            return self.handle_write_result(child);
        }
        if let Some(list) = self.pending_list.as_mut() {
            if child.id == list.porcelain_id {
                list.porcelain = Some(child.stdout);
            } else if child.id == list.ls_id {
                list.ls = Some(child.stdout);
            }
            if list.porcelain.is_some() && list.ls.is_some() {
                return self.finish_list();
            }
            return Vec::new();
        }
        let width = self.pane_width();
        if let Some(dumps) = self.pending_dumps.as_mut() {
            for part in &mut dumps.parts {
                if let DumpPart::Waiting { id, fail } = part {
                    if *id == child.id {
                        *part = if child.exit == 0 {
                            DumpPart::Ready(bytes_to_text(&child.stdout, width))
                        } else {
                            DumpPart::Ready(text_from_string(failure_text(
                                &child.stderr,
                                &child.stdout,
                                child.exit,
                                *fail,
                            )))
                        };
                        break;
                    }
                }
            }
        }
        self.try_finish_dumps();
        Vec::new()
    }

    fn finish_list(&mut self) -> Vec<Effect> {
        let list = self.pending_list.take().expect("pending list");
        let porcelain = list.porcelain.unwrap_or_default();
        let ls = list.ls.unwrap_or_default();
        self.porcelain = parse_porcelain(&porcelain);
        self.ls_files = parse_ls_files(&ls);
        let old_rows = self.rows.clone();
        let old_selected = self.selected;
        self.rebuild_rows();
        self.selected = walk_selection(&old_rows, old_selected, &self.rows);
        if matches!(self.strip, Some(Strip::NoFilesCommitError)) && !self.porcelain.is_empty() {
            self.strip = None;
        }
        let effects = self.request_dumps();
        if self.open_overlay_after_refresh {
            self.open_overlay_after_refresh = false;
            self.overlay = true;
            self.field = CommitField::Summary;
            self.strip = None;
        }
        effects
    }

    fn handle_write_result(&mut self, child: ChildResult) -> Vec<Effect> {
        let kind = self.writing.take();
        self.write_id = None;
        let ok = child.exit == 0;
        match kind {
            Some(WriteKind::Space) => {
                if ok {
                    self.start_refresh()
                } else {
                    self.strip = Some(Strip::GitWriteError {
                        text: failure_text(&child.stderr, &child.stdout, child.exit, FailKind::Git),
                    });
                    Vec::new()
                }
            }
            Some(WriteKind::AddAll) => {
                if ok {
                    self.open_overlay_after_refresh = true;
                    self.start_refresh()
                } else {
                    self.strip = Some(Strip::GitWriteError {
                        text: failure_text(&child.stderr, &child.stdout, child.exit, FailKind::Git),
                    });
                    Vec::new()
                }
            }
            Some(WriteKind::Commit) => {
                if ok {
                    self.overlay = false;
                    self.strip = None;
                    self.draft = CommitDraft::default();
                    self.summary_caret = 0;
                    self.description_caret = 0;
                    self.field = CommitField::Summary;
                    self.start_refresh()
                } else {
                    self.strip = Some(Strip::GitWriteError {
                        text: failure_text(&child.stderr, &child.stdout, child.exit, FailKind::Git),
                    });
                    Vec::new()
                }
            }
            None => Vec::new(),
        }
    }

    fn start_refresh(&mut self) -> Vec<Effect> {
        let porcelain_id = self.alloc();
        let ls_id = self.alloc();
        self.pending_list = Some(PendingList {
            porcelain_id,
            ls_id,
            porcelain: None,
            ls: None,
        });
        vec![
            Effect {
                id: porcelain_id,
                kind: EffectKind::GitPorcelain,
            },
            Effect {
                id: ls_id,
                kind: EffectKind::GitLsFiles,
            },
        ]
    }

    fn request_dumps(&mut self) -> Vec<Effect> {
        let path = self.selected_path().map(str::to_string);
        if path != self.dump_scroll_path {
            self.scroll = 0;
            self.dump_scroll_path = path;
        }
        let width = self.pane_width();
        let plan = self.current_plan();
        let mut effects = Vec::new();
        let mut parts = Vec::new();
        if plan.is_empty() {
            self.set_pane(Text::default());
            self.pending_dumps = None;
            return effects;
        }
        for req in plan {
            match req {
                DumpReq::Literal(s) => parts.push(DumpPart::Ready(text_from_string(s))),
                DumpReq::Git { path, cached } => {
                    let id = self.alloc();
                    parts.push(DumpPart::Waiting {
                        id,
                        fail: FailKind::Git,
                    });
                    effects.push(Effect {
                        id,
                        kind: EffectKind::GitDiff {
                            path,
                            cached,
                            width,
                        },
                    });
                }
                DumpReq::Difft { path } => {
                    let id = self.alloc();
                    parts.push(DumpPart::Waiting {
                        id,
                        fail: FailKind::Difft,
                    });
                    effects.push(Effect {
                        id,
                        kind: EffectKind::Difft { path, width },
                    });
                }
            }
        }
        self.pending_dumps = Some(PendingDumps { parts });
        self.try_finish_dumps();
        effects
    }

    fn current_plan(&self) -> Vec<DumpReq> {
        let Some(row) = self.selected.and_then(|i| self.rows.get(i)) else {
            return Vec::new();
        };
        if row.kind == RowKind::Directory {
            let filter = self.filter();
            let files = folder_stack(
                &self.porcelain,
                &self.ls_files,
                self.files_mode(),
                &row.path,
                filter,
            );
            if files.is_empty() {
                return vec![DumpReq::Literal("no changes".into())];
            }
            let mut plan = Vec::new();
            for (path, xy) in files {
                plan.extend(file_plan(&path, xy));
            }
            return plan;
        }
        let xy = self
            .porcelain
            .iter()
            .find(|e| e.path == row.path)
            .map(|e| e.xy);
        file_plan(&row.path, xy)
    }

    fn try_finish_dumps(&mut self) {
        let Some(dumps) = self.pending_dumps.as_ref() else {
            return;
        };
        if dumps.parts.iter().any(|p| matches!(p, DumpPart::Waiting { .. })) {
            return;
        }
        let parts: Vec<Text<'static>> = dumps
            .parts
            .iter()
            .map(|p| match p {
                DumpPart::Ready(t) => t.clone(),
                DumpPart::Waiting { .. } => Text::default(),
            })
            .collect();
        self.set_pane(stack_texts(parts));
        self.pending_dumps = None;
    }

    fn set_pane(&mut self, text: Text<'static>) {
        self.pane_body = pane_string(&text);
        self.pane_lines = text.lines.len() as u16;
        self.pane_text = text;
        self.clamp_scroll();
    }

    fn rebuild_rows(&mut self) {
        self.rows = visible_rows(
            &self.porcelain,
            &self.ls_files,
            self.files_mode(),
            &self.collapsed,
            self.filter(),
            FILES_PANEL_COLS as usize,
        );
    }

    fn rebuild_keep(&mut self) {
        let old = self.rows.clone();
        let old_sel = self.selected;
        self.rebuild_rows();
        self.selected = walk_selection(&old, old_sel, &self.rows);
        if self.rows.is_empty() {
            self.selected = None;
            self.set_pane(Text::default());
        }
    }

    fn filter(&self) -> Option<&str> {
        if self.query.is_empty() {
            None
        } else {
            Some(self.query.as_str())
        }
    }

    fn move_or_scroll(&mut self, delta: isize) -> Vec<Effect> {
        if self.focus == Focus::Diff {
            self.scroll_diff(delta);
            return Vec::new();
        }
        let Some(idx) = self.selected else {
            if !self.rows.is_empty() {
                return self.select(Some(0));
            }
            return Vec::new();
        };
        let n = self.rows.len() as isize;
        if n == 0 {
            return Vec::new();
        }
        let next = (idx as isize + delta).clamp(0, n - 1) as usize;
        if next == idx {
            Vec::new()
        } else {
            self.select(Some(next))
        }
    }

    fn select(&mut self, index: Option<usize>) -> Vec<Effect> {
        self.selected = index;
        self.request_dumps()
    }

    fn toggle_fold(&mut self) -> Vec<Effect> {
        let Some(row) = self.selected.and_then(|i| self.rows.get(i)) else {
            return Vec::new();
        };
        if row.kind != RowKind::Directory {
            return Vec::new();
        }
        let path = row.path.clone();
        if !self.collapsed.remove(&path) {
            self.collapsed.insert(path.clone());
        }
        self.rebuild_keep();
        Vec::new()
    }

    fn toggle_scope(&mut self) -> Vec<Effect> {
        self.scope = match self.scope {
            Scope::Changed => Scope::All,
            Scope::All => Scope::Changed,
        };
        self.rebuild_keep();
        self.request_dumps()
    }

    fn toggle_shape(&mut self) -> Vec<Effect> {
        if self.scope == Scope::All {
            return Vec::new();
        }
        self.shape = match self.shape {
            Shape::Flat => Shape::Tree,
            Shape::Tree => Shape::Flat,
        };
        self.rebuild_keep();
        self.request_dumps()
    }

    fn stage_selected(&mut self) -> Vec<Effect> {
        let Some(row) = self.selected.and_then(|i| self.rows.get(i)).cloned() else {
            return Vec::new();
        };
        if row.kind != RowKind::File {
            return Vec::new();
        }
        let Some(entry) = self.porcelain.iter().find(|e| e.path == row.path).cloned() else {
            return Vec::new();
        };
        let path = entry.path;
        let kind = if has_unstaged(entry.xy) {
            EffectKind::GitAdd { path }
        } else if is_in_head(entry.xy) {
            EffectKind::GitReset { path }
        } else {
            EffectKind::GitRmCached { path }
        };
        self.begin_write(WriteKind::Space, kind)
    }

    fn open_commit(&mut self) -> Vec<Effect> {
        if self.porcelain.is_empty() {
            self.strip = Some(Strip::NoFilesCommitError);
            return Vec::new();
        }
        let staged = self.porcelain.iter().any(|e| {
            row_color(e.xy) == crate::event::RowColor::Staged
                || row_color(e.xy) == crate::event::RowColor::Both
        });
        if !staged {
            self.strip = Some(Strip::NothingStagedConfirm);
            return Vec::new();
        }
        self.overlay = true;
        self.field = CommitField::Summary;
        Vec::new()
    }

    fn git_add_all(&mut self) -> Vec<Effect> {
        self.begin_write(WriteKind::AddAll, EffectKind::GitAddAll)
    }

    fn try_commit(&mut self) -> Vec<Effect> {
        if self.draft.summary.trim().is_empty() {
            self.strip = Some(Strip::EmptySummaryRefusal);
            return Vec::new();
        }
        self.strip = None;
        let summary = self.draft.summary.clone();
        let description = self.draft.description.clone();
        self.begin_write(
            WriteKind::Commit,
            EffectKind::GitCommit {
                summary,
                description,
            },
        )
    }

    fn begin_write(&mut self, kind: WriteKind, effect: EffectKind) -> Vec<Effect> {
        let id = self.alloc();
        self.writing = Some(kind);
        self.write_id = Some(id);
        vec![Effect { id, kind: effect }]
    }

    fn scroll_diff(&mut self, delta: isize) {
        let next = self.scroll as isize + delta;
        self.scroll = next.clamp(0, i32::MAX as isize) as u16;
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let visible = self
            .term_rows
            .saturating_sub(1)
            .saturating_sub(self.overlay_height())
            .max(1);
        let max = self.pane_lines.saturating_sub(visible);
        if self.scroll > max {
            self.scroll = max;
        }
    }

    fn page(&self) -> u16 {
        self.rows_inner().max(1)
    }

    fn rows_inner(&self) -> u16 {
        self.term_rows
            .saturating_sub(2)
            .saturating_sub(self.overlay_height())
            .max(1)
    }

    fn field_width(&self) -> u16 {
        self.cols.saturating_sub(2).max(1)
    }

    fn nudge_caret(&mut self, delta: isize) {
        match self.field {
            CommitField::Summary => {
                move_caret(&mut self.summary_caret, char_len(&self.draft.summary), delta)
            }
            CommitField::Description => move_caret(
                &mut self.description_caret,
                char_len(&self.draft.description),
                delta,
            ),
        }
    }

    fn alloc(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    fn effect(&mut self, kind: EffectKind) -> Effect {
        Effect {
            id: self.alloc(),
            kind,
        }
    }
}

fn file_plan(path: &str, xy: Option<[char; 2]>) -> Vec<DumpReq> {
    let Some(xy) = xy else {
        return vec![DumpReq::Literal(format!("{path} --- no changes"))];
    };
    if is_untracked(xy) {
        return vec![DumpReq::Difft {
            path: path.to_string(),
        }];
    }
    let mut plan = Vec::new();
    if has_unstaged(xy) {
        plan.push(DumpReq::Git {
            path: path.to_string(),
            cached: false,
        });
    }
    if has_staged(xy) {
        plan.push(DumpReq::Git {
            path: path.to_string(),
            cached: true,
        });
    }
    if plan.is_empty() {
        plan.push(DumpReq::Literal(format!("{path} --- no changes")));
    }
    plan
}

fn walk_selection(
    old_rows: &[FilesRow],
    old_selected: Option<usize>,
    new_rows: &[FilesRow],
) -> Option<usize> {
    if new_rows.is_empty() {
        return None;
    }
    let Some(old_idx) = old_selected else {
        return Some(0);
    };
    let old_path = old_rows.get(old_idx).map(|r| r.path.as_str());
    if let Some(path) = old_path {
        if let Some(i) = new_rows.iter().position(|r| r.path == path) {
            return Some(i);
        }
    }
    for row in old_rows.iter().skip(old_idx.saturating_add(1)) {
        if let Some(i) = new_rows.iter().position(|r| r.path == row.path) {
            return Some(i);
        }
    }
    for row in old_rows.iter().take(old_idx).rev() {
        if let Some(i) = new_rows.iter().position(|r| r.path == row.path) {
            return Some(i);
        }
    }
    Some(0)
}
