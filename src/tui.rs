//! Terminal adapter: alternate screen, keys, mouse, and child processes.

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, stdout, Read};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event as TermEvent, KeyCode, KeyEvent,
    KeyEventKind, KeyModifiers, KeyboardEnhancementFlags, MouseButton, MouseEvent, MouseEventKind,
    PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
};
use crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use crate::event::{
    ChildResult, CommitField, Effect, EffectKind, Event, Focus, Key, MouseHit, RowColor,
};
use crate::session::{apply, Session};

const STAGED: Color = Color::Rgb(122, 186, 122);
const BOTH: Color = Color::Rgb(214, 168, 88);
const BRASS: Color = Color::Rgb(232, 196, 104);
const MUTED: Color = Color::Rgb(110, 110, 110);
const POLL: Duration = Duration::from_secs(10);
const DUMP_DEBOUNCE: Duration = Duration::from_millis(50);
const REAP: Duration = Duration::from_millis(16);
/// Columns the footer search field reserves for its `" /"` prefix.
const SEARCH_PREFIX: u16 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum DumpSide {
    Unstaged,
    Staged,
    Untracked,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct DumpKey {
    path: String,
    side: DumpSide,
    width: u16,
    hash: String,
}

struct DumpJobs {
    tx: Sender<ChildResult>,
    rx: Receiver<ChildResult>,
    pids: Vec<(u64, u32)>,
    delayed: Option<(Instant, Vec<Effect>)>,
    cache: HashMap<DumpKey, Vec<u8>>,
    in_flight: HashMap<u64, DumpKey>,
}

impl DumpJobs {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            tx,
            rx,
            pids: Vec::new(),
            delayed: None,
            cache: HashMap::new(),
            in_flight: HashMap::new(),
        }
    }

    fn cached(&self, effect: &Effect) -> Option<Vec<u8>> {
        dump_key(effect).and_then(|key| self.cache.get(&key).cloned())
    }

    fn schedule(&mut self, dumps: Vec<Effect>) {
        self.cancel();
        self.delayed = Some((Instant::now() + DUMP_DEBOUNCE, dumps));
    }

    fn cancel(&mut self) {
        self.delayed = None;
        for (_, pid) in self.pids.drain(..) {
            terminate(pid);
        }
    }

    fn flush_due(&mut self) {
        let Some((when, dumps)) = self.delayed.take() else {
            return;
        };
        if Instant::now() < when {
            self.delayed = Some((when, dumps));
            return;
        }
        for effect in dumps {
            if let Some(stdout) = self.cached(&effect) {
                let _ = self.tx.send(ChildResult {
                    id: effect.id,
                    exit: 0,
                    stdout,
                    stderr: Vec::new(),
                });
                continue;
            }
            self.spawn(effect);
        }
    }

    fn spawn(&mut self, effect: Effect) {
        if let Some(key) = dump_key(&effect) {
            self.in_flight.insert(effect.id, key.clone());
        }
        if let Some(stdout) = dump_skip(&effect) {
            self.store(effect.id, &stdout);
            let _ = self.tx.send(ChildResult {
                id: effect.id,
                exit: 0,
                stdout,
                stderr: Vec::new(),
            });
            return;
        }
        let Some(mut cmd) = dump_command(&effect) else {
            return;
        };
        match cmd.spawn() {
            Ok(child) => {
                let pid = child.id();
                let id = effect.id;
                let tx = self.tx.clone();
                self.pids.push((id, pid));
                thread::spawn(move || {
                    let _ = tx.send(wait_child(id, child));
                });
            }
            Err(err) => {
                self.in_flight.remove(&effect.id);
                let _ = self.tx.send(ChildResult {
                    id: effect.id,
                    exit: 1,
                    stdout: Vec::new(),
                    stderr: format!("{err}\n").into_bytes(),
                });
            }
        }
    }

    fn store(&mut self, id: u64, stdout: &[u8]) {
        if let Some(key) = self.in_flight.remove(&id) {
            self.cache.insert(key, stdout.to_vec());
        }
    }

    fn reap(&mut self) -> Vec<ChildResult> {
        let mut out = Vec::new();
        while let Ok(result) = self.rx.try_recv() {
            self.pids.retain(|(id, _)| *id != result.id);
            if result.exit == 0 {
                self.store(result.id, &result.stdout);
            } else {
                self.in_flight.remove(&result.id);
            }
            out.push(result);
        }
        out
    }

    fn wait(&self, tick: Instant) -> Duration {
        let mut wait = tick.saturating_duration_since(Instant::now());
        if let Some((when, _)) = self.delayed {
            wait = wait.min(when.saturating_duration_since(Instant::now()));
        }
        if !self.pids.is_empty() {
            wait = wait.min(REAP);
        }
        wait
    }
}

fn terminate(pid: u32) {
    let _ = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn dump_key(effect: &Effect) -> Option<DumpKey> {
    let (path, side, width) = match &effect.kind {
        EffectKind::GitDiff {
            path,
            cached,
            width,
        } => (
            path.clone(),
            if *cached {
                DumpSide::Staged
            } else {
                DumpSide::Unstaged
            },
            *width,
        ),
        EffectKind::Difft { path, width } => (path.clone(), DumpSide::Untracked, *width),
        _ => return None,
    };
    Some(DumpKey {
        hash: content_hash(&path, side),
        path,
        side,
        width,
    })
}

fn content_hash(path: &str, side: DumpSide) -> String {
    match side {
        DumpSide::Untracked => worktree_oid(path),
        DumpSide::Unstaged => format!("{}:{}", index_oid(path), worktree_oid(path)),
        DumpSide::Staged => format!("{}:{}", head_oid(path), index_oid(path)),
    }
}

fn git_oid(args: &[&str]) -> String {
    match Command::new("git").args(args).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    }
}

fn worktree_oid(path: &str) -> String {
    if !Path::new(path).is_file() {
        return String::new();
    }
    git_oid(&["hash-object", "--", path])
}

fn index_oid(path: &str) -> String {
    git_oid(&["rev-parse", "--verify", "--quiet", &format!(":{path}")])
}

fn head_oid(path: &str) -> String {
    git_oid(&["rev-parse", "--verify", "--quiet", &format!("HEAD:{path}")])
}

fn dump_skip(effect: &Effect) -> Option<Vec<u8>> {
    match &effect.kind {
        EffectKind::GitDiff { path, .. } | EffectKind::Difft { path, .. } => {
            large_binary_stdout(path)
        }
        _ => None,
    }
}

fn dump_command(effect: &Effect) -> Option<Command> {
    match &effect.kind {
        EffectKind::GitDiff {
            path,
            cached,
            width,
        } => {
            let mut args = vec!["-c", "diff.external=difft", "--no-pager", "diff"];
            if *cached {
                args.push("--cached");
            }
            args.push("--");
            args.push(path);
            let mut cmd = Command::new("git");
            cmd.args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            set_dft(&mut cmd, *width);
            Some(cmd)
        }
        EffectKind::Difft { path, width } => {
            let mut cmd = Command::new("difft");
            cmd.arg("/dev/null")
                .arg(path)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
            set_dft(&mut cmd, *width);
            Some(cmd)
        }
        _ => None,
    }
}

fn wait_child(id: u64, child: std::process::Child) -> ChildResult {
    match child.wait_with_output() {
        Ok(out) => ChildResult {
            id,
            exit: out.status.code().unwrap_or(1),
            stdout: out.stdout,
            stderr: out.stderr,
        },
        Err(err) => ChildResult {
            id,
            exit: 1,
            stdout: Vec::new(),
            stderr: format!("{err}\n").into_bytes(),
        },
    }
}

#[derive(Default)]
struct Hits {
    files_inner: Option<Rect>,
    search: Option<Rect>,
    summary: Option<Rect>,
    description: Option<Rect>,
}

pub fn run() -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let mut out = stdout();
    execute!(
        out,
        EnableMouseCapture,
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )?;
    let quit = Arc::new(AtomicBool::new(false));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGINT, Arc::clone(&quit));
    let _ = signal_hook::flag::register(signal_hook::consts::SIGTERM, Arc::clone(&quit));
    let result = run_loop(&mut terminal, &quit);
    let _ = execute!(
        out,
        DisableMouseCapture,
        PopKeyboardEnhancementFlags
    );
    ratatui::try_restore()?;
    result
}

fn run_loop(terminal: &mut DefaultTerminal, quit: &AtomicBool) -> io::Result<()> {
    let size = terminal.size()?;
    let mut jobs = DumpJobs::new();
    let (session, effects) = Session::boot(size.width, size.height);
    let mut session = match fulfill(session, effects, &mut jobs)? {
        Some(session) => session,
        None => return Ok(()),
    };
    let mut hits = Hits::default();
    let mut next_tick = Instant::now() + POLL;
    loop {
        if quit.load(Ordering::Relaxed) {
            break;
        }
        terminal.draw(|frame| {
            hits = Hits::default();
            draw(frame, &session, &mut hits);
        })?;
        jobs.flush_due();
        match pump_jobs(session, &mut jobs)? {
            Some(next) => session = next,
            None => break,
        }
        let timeout = jobs.wait(next_tick);
        if event::poll(timeout)? {
            match event::read()? {
                TermEvent::Key(key) if key.kind == KeyEventKind::Press => {
                    if let Some(key) = map_key(key) {
                        let (next, effects) = apply(session, Event::Key(key));
                        match fulfill(next, effects, &mut jobs)? {
                            Some(next) => session = next,
                            None => break,
                        }
                    }
                }
                TermEvent::Mouse(mouse) => {
                    if let Some(hit) = map_mouse(mouse, &hits, &session) {
                        let (next, effects) = apply(session, Event::Mouse(hit));
                        match fulfill(next, effects, &mut jobs)? {
                            Some(next) => session = next,
                            None => break,
                        }
                    }
                }
                TermEvent::Resize(cols, rows) => {
                    let (next, effects) = apply(session, Event::Resize { cols, rows });
                    match fulfill(next, effects, &mut jobs)? {
                        Some(next) => session = next,
                        None => break,
                    }
                }
                _ => {}
            }
        } else {
            jobs.flush_due();
            match pump_jobs(session, &mut jobs)? {
                Some(next) => session = next,
                None => break,
            }
            if Instant::now() >= next_tick {
                let (next, effects) = apply(session, Event::Tick);
                match fulfill(next, effects, &mut jobs)? {
                    Some(next) => session = next,
                    None => break,
                }
                next_tick = Instant::now() + POLL;
            }
        }
        if quit.load(Ordering::Relaxed) {
            break;
        }
    }
    jobs.cancel();
    Ok(())
}

fn pump_jobs(mut session: Session, jobs: &mut DumpJobs) -> io::Result<Option<Session>> {
    for child in jobs.reap() {
        let (next, more) = apply(session, Event::Child(child));
        match fulfill(next, more, jobs)? {
            Some(next) => session = next,
            None => return Ok(None),
        }
    }
    Ok(Some(session))
}

fn fulfill(
    mut session: Session,
    mut effects: Vec<Effect>,
    jobs: &mut DumpJobs,
) -> io::Result<Option<Session>> {
    let mut dumps = Vec::new();
    while let Some(effect) = effects.first().cloned() {
        effects.remove(0);
        match effect.kind {
            EffectKind::Quit => {
                jobs.cancel();
                return Ok(None);
            }
            EffectKind::GitDiff { .. } | EffectKind::Difft { .. } => dumps.push(effect),
            _ => {
                let child = run_effect(&effect);
                let (next, more) = apply(session, Event::Child(child));
                session = next;
                effects.extend(more);
            }
        }
    }
    let mut misses = Vec::new();
    for effect in dumps {
        if let Some(stdout) = jobs.cached(&effect) {
            let (next, more) = apply(
                session,
                Event::Child(ChildResult {
                    id: effect.id,
                    exit: 0,
                    stdout,
                    stderr: Vec::new(),
                }),
            );
            session = next;
            effects.extend(more);
        } else {
            misses.push(effect);
        }
    }
    if !misses.is_empty() {
        jobs.schedule(misses);
    }
    Ok(Some(session))
}

fn run_effect(effect: &Effect) -> ChildResult {
    match &effect.kind {
        EffectKind::Quit => ChildResult {
            id: effect.id,
            exit: 0,
            stdout: Vec::new(),
            stderr: Vec::new(),
        },
        EffectKind::GitPorcelain => git(
            effect.id,
            &["status", "--porcelain", "-z", "--untracked-files=all"],
            None,
        ),
        EffectKind::GitLsFiles => git(
            effect.id,
            &["ls-files", "-c", "-o", "--exclude-standard", "-z"],
            None,
        ),
        EffectKind::GitDiff {
            path,
            cached,
            width,
        } => {
            if let Some(stdout) = large_binary_stdout(path) {
                return ChildResult {
                    id: effect.id,
                    exit: 0,
                    stdout,
                    stderr: Vec::new(),
                };
            }
            let mut args = vec!["-c", "diff.external=difft", "--no-pager", "diff"];
            if *cached {
                args.push("--cached");
            }
            args.push("--");
            args.push(path);
            git(effect.id, &args, Some(*width))
        }
        EffectKind::Difft { path, width } => {
            if let Some(stdout) = large_binary_stdout(path) {
                return ChildResult {
                    id: effect.id,
                    exit: 0,
                    stdout,
                    stderr: Vec::new(),
                };
            }
            difft(effect.id, path, *width)
        }
        EffectKind::GitAdd { path } => git(effect.id, &["add", "--", path], None),
        EffectKind::GitReset { path } => git(effect.id, &["reset", "HEAD", "--", path], None),
        EffectKind::GitRmCached { path } => {
            git(effect.id, &["rm", "--cached", "--force", "--", path], None)
        }
        EffectKind::GitAddAll => git(effect.id, &["add", "-A"], None),
        EffectKind::GitCommit {
            summary,
            description,
        } => {
            let mut args = vec!["commit", "-m", summary.as_str()];
            if !description.is_empty() {
                args.push("-m");
                args.push(description.as_str());
            }
            git(effect.id, &args, None)
        }
    }
}

fn git(id: u64, args: &[&str], width: Option<u16>) -> ChildResult {
    let mut cmd = Command::new("git");
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(width) = width {
        set_dft(&mut cmd, width);
    }
    output_to_child(id, cmd)
}

fn difft(id: u64, path: &str, width: u16) -> ChildResult {
    let mut cmd = Command::new("difft");
    cmd.arg("/dev/null")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    set_dft(&mut cmd, width);
    output_to_child(id, cmd)
}

fn set_dft(cmd: &mut Command, width: u16) {
    cmd.env("DFT_WIDTH", width.to_string())
        .env("DFT_COLOR", "always")
        .env("DFT_DISPLAY", "side-by-side")
        .env("DFT_BACKGROUND", "dark");
}

const LARGE_BINARY_BYTES: u64 = 1_000_000;
const BINARY_SNIFF: usize = 8000;

fn large_binary_stdout(path: &str) -> Option<Vec<u8>> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() < LARGE_BINARY_BYTES {
        return None;
    }
    let mut file = File::open(path).ok()?;
    let mut buf = [0u8; BINARY_SNIFF];
    let n = file.read(&mut buf).ok()?;
    if !buf[..n].contains(&0) {
        return None;
    }
    Some(b"Binary contents changed.\n".to_vec())
}

fn output_to_child(id: u64, mut cmd: Command) -> ChildResult {
    match cmd.output() {
        Ok(out) => ChildResult {
            id,
            exit: out.status.code().unwrap_or(1),
            stdout: out.stdout,
            stderr: out.stderr,
        },
        Err(err) => ChildResult {
            id,
            exit: 1,
            stdout: Vec::new(),
            stderr: format!("{err}\n").into_bytes(),
        },
    }
}

fn map_key(key: KeyEvent) -> Option<Key> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    Some(match key.code {
        KeyCode::Char(c) => Key::Char {
            c,
            ctrl: ctrl || key.modifiers.contains(KeyModifiers::CONTROL),
        },
        KeyCode::Enter => Key::Enter { ctrl },
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        _ => return None,
    })
}

fn map_mouse(mouse: MouseEvent, hits: &Hits, session: &Session) -> Option<MouseHit> {
    if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
        return None;
    }
    let pos = Position::new(mouse.column, mouse.row);
    if let Some(area) = hits.summary {
        if area.contains(pos) {
            return Some(MouseHit::Summary {
                col: pos.x.saturating_sub(area.x),
            });
        }
    }
    if let Some(area) = hits.description {
        if area.contains(pos) {
            return Some(MouseHit::Description {
                col: pos.x.saturating_sub(area.x),
                row: pos.y.saturating_sub(area.y),
            });
        }
    }
    if let Some(area) = hits.search {
        if area.contains(pos) {
            return Some(MouseHit::Search {
                col: pos.x.saturating_sub(area.x),
            });
        }
    }
    if let Some(area) = hits.files_inner {
        if area.contains(pos) {
            let index = pos.y.saturating_sub(area.y) as usize;
            if index < session.files_rows().len() {
                return Some(MouseHit::Files { index });
            }
        }
    }
    None
}

fn draw(frame: &mut Frame, session: &Session, hits: &mut Hits) {
    let [body, strip, footer] = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(session.overlay_height()),
        Constraint::Length(1),
    ])
    .areas(frame.area());
    let files_w = session.files_panel_cols();
    let [files, diff] =
        Layout::horizontal([Constraint::Length(files_w), Constraint::Fill(1)]).areas(body);

    let files_block = Block::new()
        .borders(Borders::RIGHT)
        .title(mode_title(session))
        .style(if session.focus() == Focus::Files {
            Style::default().fg(BRASS)
        } else {
            Style::default().fg(MUTED)
        });
    let inner = files_block.inner(files);
    hits.files_inner = Some(inner);
    let items: Vec<ListItem> = session
        .files_rows()
        .iter()
        .map(|row| {
            let color = match row.color {
                RowColor::Staged => STAGED,
                RowColor::Both => BOTH,
                RowColor::Default => Color::Reset,
            };
            ListItem::new(Line::from(Span::styled(
                row.display.clone(),
                Style::default().fg(color),
            )))
        })
        .collect();
    let mut state = ListState::default();
    state.select(session.selected_index());
    frame.render_stateful_widget(
        List::new(items)
            .block(files_block)
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
        files,
        &mut state,
    );

    frame.render_widget(diff_pane(session), diff);

    if session.overlay_open() {
        draw_overlay(frame, session, strip, hits);
    } else if let Some(text) = session.strip_text() {
        frame.render_widget(
            Paragraph::new(text).block(Block::new().borders(Borders::TOP)),
            strip,
        );
    }

    if session.search_open() {
        let search = format!(" /{} ", session.search_query());
        let field = Rect {
            x: footer.x.saturating_add(SEARCH_PREFIX),
            y: footer.y,
            width: footer.width.saturating_sub(SEARCH_PREFIX),
            height: footer.height,
        };
        hits.search = Some(field);
        frame.render_widget(
            Paragraph::new(search).style(Style::default().fg(BRASS)),
            footer,
        );
        let caret = session.caret() as u16;
        frame.set_cursor_position(Position::new(
            field.x + caret.min(field.width.saturating_sub(1)),
            field.y,
        ));
    } else {
        frame.render_widget(Paragraph::new(session.footer_text()).style(Style::default().fg(MUTED)), footer);
    }
}

fn diff_pane(session: &Session) -> Paragraph<'static> {
    if session.dumps_pending() {
        let hint = session.selected_path().unwrap_or("diff");
        Paragraph::new(format!(
            " {}  loading {hint}…\n  difft is working",
            spinner_frame()
        ))
        .style(Style::default().fg(BRASS))
    } else {
        Paragraph::new(session.pane_text().clone()).scroll((session.diff_scroll(), 0))
    }
}

fn spinner_frame() -> char {
    const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    let ticks = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() / 80)
        .unwrap_or(0) as usize;
    FRAMES[ticks % FRAMES.len()]
}

fn mode_title(session: &Session) -> &'static str {
    match session.files_mode() {
        crate::event::FilesMode::ChangedFlat => " changed ",
        crate::event::FilesMode::ChangedTree => " changed · tree ",
        crate::event::FilesMode::AllTree => " all files ",
    }
}

fn draw_overlay(frame: &mut Frame, session: &Session, area: Rect, hits: &mut Hits) {
    let error_h = match session.strip() {
        Some(crate::event::Strip::GitWriteError { text }) => {
            (text.split('\n').count() as u16).min(8).max(1)
        }
        Some(crate::event::Strip::EmptySummaryRefusal) => 1,
        _ => 0,
    };
    let [err, form] = Layout::vertical([Constraint::Length(error_h), Constraint::Fill(1)]).areas(area);
    if error_h > 0 {
        if let Some(text) = session.strip_text() {
            frame.render_widget(
                Paragraph::new(text).style(Style::default().fg(Color::Red)),
                err,
            );
        }
    }
    let block = Block::new()
        .borders(Borders::ALL)
        .title(" commit ")
        .style(Style::default().fg(BRASS));
    let inner = block.inner(form);
    frame.render_widget(block, form);
    let [sum, desc] = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(inner);
    hits.summary = Some(sum);
    hits.description = Some(desc);
    let draft = session.draft();
    let sum_style = field_style(session.commit_field() == Some(CommitField::Summary));
    let desc_style = field_style(session.commit_field() == Some(CommitField::Description));
    frame.render_widget(
        Paragraph::new(if draft.summary.is_empty() {
            "summary".into()
        } else {
            draft.summary.clone()
        })
        .style(sum_style),
        sum,
    );
    frame.render_widget(
        Paragraph::new(if draft.description.is_empty() {
            "description".into()
        } else {
            draft.description.clone()
        })
        .style(desc_style),
        desc,
    );
    let caret = session.caret();
    if session.commit_field() == Some(CommitField::Summary) {
        frame.set_cursor_position(Position::new(
            sum.x + (caret as u16).min(sum.width.saturating_sub(1)),
            sum.y,
        ));
    } else if session.commit_field() == Some(CommitField::Description) {
        frame.set_cursor_position(Position::new(
            desc.x + (caret as u16).min(desc.width.saturating_sub(1)),
            desc.y,
        ));
    }
}

fn field_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(BRASS).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(MUTED)
    }
}

#[cfg(test)]
mod tests {
    use ratatui::backend::TestBackend;
    use ratatui::style::Modifier;
    use ratatui::Terminal;

    use super::*;
    use crate::event::{ChildResult, Effect, EffectKind, Event, Key};

    fn porcelain_z(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        for (xy, path) in entries {
            out.extend_from_slice(xy.as_bytes());
            out.push(b' ');
            out.extend_from_slice(path.as_bytes());
            out.push(0);
        }
        out
    }

    fn session_with_files() -> Session {
        let (mut session, effects) = Session::boot(80, 24);
        let status = effects
            .iter()
            .find(|e| matches!(e.kind, EffectKind::GitPorcelain))
            .unwrap()
            .id;
        let ls = effects
            .iter()
            .find(|e| matches!(e.kind, EffectKind::GitLsFiles))
            .unwrap()
            .id;
        let (next, _) = apply(
            session,
            Event::Child(ChildResult {
                id: status,
                exit: 0,
                stdout: porcelain_z(&[(" M", "a.rs"), (" M", "b.rs")]),
                stderr: Vec::new(),
            }),
        );
        session = next;
        let (next, more) = apply(
            session,
            Event::Child(ChildResult {
                id: ls,
                exit: 0,
                stdout: Vec::new(),
                stderr: Vec::new(),
            }),
        );
        session = next;
        for effect in more {
            if matches!(
                effect.kind,
                EffectKind::GitDiff { .. } | EffectKind::Difft { .. }
            ) {
                let (next, _) = apply(
                    session,
                    Event::Child(ChildResult {
                        id: effect.id,
                        exit: 0,
                        stdout: b"dump\n".to_vec(),
                        stderr: Vec::new(),
                    }),
                );
                session = next;
            }
        }
        session
    }

    fn render(session: &Session) -> ratatui::buffer::Buffer {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                let mut hits = Hits::default();
                draw(frame, session, &mut hits);
            })
            .unwrap();
        terminal.backend().buffer().clone()
    }

    fn render_hits(session: &Session) -> (Hits, Option<Position>) {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut hits = Hits::default();
        terminal
            .draw(|frame| {
                draw(frame, session, &mut hits);
            })
            .unwrap();
        let cursor = terminal.get_cursor_position().ok();
        (hits, cursor)
    }

    fn row_reversed(buf: &ratatui::buffer::Buffer, needle: &str) -> bool {
        for y in 0..buf.area.height {
            let mut line = String::new();
            for x in 0..28.min(buf.area.width) {
                line.push_str(buf[(x, y)].symbol());
            }
            if !line.contains(needle) {
                continue;
            }
            return (0..28.min(buf.area.width))
                .any(|x| buf[(x, y)].modifier.contains(Modifier::REVERSED));
        }
        panic!("files panel missing {needle}");
    }

    #[test]
    fn selected_files_row_is_reversed() {
        let session = session_with_files();
        assert_eq!(session.selected_path(), Some("a.rs"));
        let buf = render(&session);
        assert!(row_reversed(&buf, "a.rs"), "selected a.rs should reverse");
        assert!(!row_reversed(&buf, "b.rs"), "unselected b.rs should not reverse");

        let (session, _) = apply(session, Event::Key(Key::Char { c: 'j', ctrl: false }));
        assert_eq!(session.selected_path(), Some("b.rs"));
        let buf = render(&session);
        assert!(row_reversed(&buf, "b.rs"), "selected b.rs should reverse");
        assert!(!row_reversed(&buf, "a.rs"), "unselected a.rs should not reverse");
    }

    #[test]
    fn pending_dump_renders_loading_in_the_diff_pane() {
        let session = session_with_files();
        let (session, _) = apply(session, Event::Key(Key::Char { c: 'j', ctrl: false }));
        assert!(session.dumps_pending());
        let buf = render(&session);
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(text.contains("loading b.rs"), "{text}");
        assert!(text.contains("difft is working"), "{text}");
    }

    #[test]
    fn worktree_hash_changes_when_file_bytes_change() {
        let dir = std::env::temp_dir().join("difforge-dump-hash");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("n.rs");
        std::fs::write(&path, b"alpha").unwrap();
        let p = path.to_str().unwrap();
        let first = content_hash(p, DumpSide::Untracked);
        std::fs::write(&path, b"beta").unwrap();
        let second = content_hash(p, DumpSide::Untracked);
        assert_ne!(first, second);
        assert_eq!(first.len(), 40, "git hash-object sha1, got {first:?}");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn dump_cache_returns_stored_stdout_for_the_same_key() {
        let mut jobs = DumpJobs::new();
        let effect = Effect {
            id: 1,
            kind: EffectKind::Difft {
                path: "gone.rs".into(),
                width: 80,
            },
        };
        let key = dump_key(&effect).unwrap();
        jobs.cache.insert(key, b"hit\n".to_vec());
        assert_eq!(jobs.cached(&effect).as_deref(), Some(b"hit\n".as_slice()));
        let other = Effect {
            id: 2,
            kind: EffectKind::Difft {
                path: "gone.rs".into(),
                width: 81,
            },
        };
        assert!(jobs.cached(&other).is_none());
    }

    #[test]
    fn cache_hit_finishes_dumps_without_loading_or_spawn() {
        let session = session_with_files();
        let (session, effects) = apply(session, Event::Key(Key::Char { c: 'j', ctrl: false }));
        assert!(session.dumps_pending());
        let mut jobs = DumpJobs::new();
        for effect in &effects {
            if let Some(key) = dump_key(effect) {
                jobs.cache.insert(key, b"from cache\n".to_vec());
            }
        }
        let session = fulfill(session, effects, &mut jobs)
            .unwrap()
            .expect("session");
        assert!(!session.dumps_pending());
        assert!(
            session.pane_body().contains("from cache"),
            "got {:?}",
            session.pane_body()
        );
        assert!(jobs.delayed.is_none());
        let buf = render(&session);
        let mut text = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                text.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(!text.contains("loading"), "{text}");
        assert!(!text.contains("difft is working"), "{text}");
    }

    #[test]
    fn dump_schedule_keeps_only_the_latest_batch() {
        let mut jobs = DumpJobs::new();
        jobs.schedule(vec![Effect {
            id: 1,
            kind: EffectKind::Difft {
                path: "a.rs".into(),
                width: 80,
            },
        }]);
        jobs.schedule(vec![Effect {
            id: 2,
            kind: EffectKind::Difft {
                path: "b.rs".into(),
                width: 80,
            },
        }]);
        let ids: Vec<u64> = jobs
            .delayed
            .as_ref()
            .unwrap()
            .1
            .iter()
            .map(|e| e.id)
            .collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn large_binary_path_is_a_one_liner_without_spawning_difft() {
        let dir = std::env::temp_dir().join("difforge-large-binary-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blob.bin");
        std::fs::write(&path, vec![0u8; 1_200_000]).unwrap();
        let t0 = Instant::now();
        let out = large_binary_stdout(path.to_str().unwrap()).expect("large NUL file");
        assert!(
            t0.elapsed() < Duration::from_millis(100),
            "sniff took {:?}",
            t0.elapsed()
        );
        assert_eq!(out, b"Binary contents changed.\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn search_field_maps_clicks_and_draws_the_caret_past_the_prefix() {
        let session = session_with_files();
        let (mut session, _) = apply(session, Event::Key(Key::Char { c: '/', ctrl: false }));
        for c in "abc".chars() {
            let (next, _) = apply(session, Event::Key(Key::Char { c, ctrl: false }));
            session = next;
        }

        let (hits, cursor) = render_hits(&session);
        let area = hits.search.expect("search hit area");
        assert_eq!(area.x, SEARCH_PREFIX, "query starts after the ` /` prefix");

        // Clicking the query's second visible column resolves to caret index 1.
        let hit = map_mouse(
            MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: area.x + 1,
                row: area.y,
                modifiers: KeyModifiers::NONE,
            },
            &hits,
            &session,
        );
        assert_eq!(hit, Some(MouseHit::Search { col: 1 }));

        // The caret sits at the end of "abc", offset past the prefix.
        assert_eq!(cursor, Some(Position::new(area.x + 3, area.y)));
    }
}
