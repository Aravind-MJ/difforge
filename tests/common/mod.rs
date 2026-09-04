#![allow(dead_code)]

use difforge::{apply, ChildResult, Effect, EffectKind, Event, Key, MouseHit, Session};

pub const COLS: u16 = 80;
pub const ROWS: u16 = 24;

pub fn porcelain_z(entries: &[(&str, &str)]) -> Vec<u8> {
    let mut out = Vec::new();
    for (xy, path) in entries {
        assert_eq!(xy.chars().count(), 2, "porcelain XY must be two characters");
        out.extend_from_slice(xy.as_bytes());
        out.push(b' ');
        out.extend_from_slice(path.as_bytes());
        out.push(0);
    }
    out
}

pub fn ls_files_z(paths: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    for path in paths {
        out.extend_from_slice(path.as_bytes());
        out.push(0);
    }
    out
}

pub fn ok_child(id: u64, stdout: impl Into<Vec<u8>>) -> ChildResult {
    ChildResult {
        id,
        exit: 0,
        stdout: stdout.into(),
        stderr: Vec::new(),
    }
}

pub fn fail_child(id: u64, exit: i32, stderr: impl Into<Vec<u8>>, stdout: impl Into<Vec<u8>>) -> ChildResult {
    ChildResult {
        id,
        exit,
        stdout: stdout.into(),
        stderr: stderr.into(),
    }
}

pub fn take(effects: &[Effect], kind_matches: impl Fn(&EffectKind) -> bool) -> &Effect {
    effects
        .iter()
        .find(|e| kind_matches(&e.kind))
        .expect("missing effect")
}

pub fn feed(session: Session, child: ChildResult) -> (Session, Vec<Effect>) {
    apply(session, Event::Child(child))
}

pub fn key(session: Session, key: Key) -> (Session, Vec<Effect>) {
    apply(session, Event::Key(key))
}

pub fn ch(c: char) -> Key {
    Key::Char { c, ctrl: false }
}

pub fn ctrl(c: char) -> Key {
    Key::Char { c, ctrl: true }
}

/// Boot, fulfill porcelain + ls-files, then fulfill every dump the Session asked for.
pub fn session_with(
    porcelain: &[(&str, &str)],
    ls_files: &[&str],
    dump: impl Fn(&EffectKind) -> Vec<u8>,
) -> Session {
    let (session, effects) = Session::boot(COLS, ROWS);
    let (session, effects) = finish_refresh(session, effects, porcelain, ls_files, dump);
    session
}

pub fn tall_dump() -> Vec<u8> {
    (0..40).map(|i| format!("line {i}\n")).collect::<String>().into_bytes()
}

pub fn session_changed(porcelain: &[(&str, &str)]) -> Session {
    session_with(porcelain, &[], |_| tall_dump())
}

pub fn finish_refresh(
    mut session: Session,
    mut effects: Vec<Effect>,
    porcelain: &[(&str, &str)],
    ls_files: &[&str],
    dump: impl Fn(&EffectKind) -> Vec<u8>,
) -> (Session, Vec<Effect>) {
    let status = take(&effects, |k| matches!(k, EffectKind::GitPorcelain)).id;
    let ls = take(&effects, |k| matches!(k, EffectKind::GitLsFiles)).id;
    let (next, more) = feed(session, ok_child(status, porcelain_z(porcelain)));
    session = next;
    effects = more;
    let (next, more) = feed(session, ok_child(ls, ls_files_z(ls_files)));
    session = next;
    effects.extend(more);

    let dumps: Vec<Effect> = effects
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EffectKind::GitDiff { .. } | EffectKind::Difft { .. }
            )
        })
        .cloned()
        .collect();
    let requested = dumps.clone();
    for effect in dumps {
        let body = dump(&effect.kind);
        let (next, more) = feed(session, ok_child(effect.id, body));
        session = next;
        effects = more;
    }
    effects.extend(requested);
    (session, effects)
}

pub fn pane_width() -> u16 {
    COLS.saturating_sub(28)
}

pub fn finish_dumps(
    mut session: Session,
    mut effects: Vec<Effect>,
    dump: impl Fn(&EffectKind) -> Vec<u8>,
) -> (Session, Vec<Effect>) {
    let dumps: Vec<Effect> = effects
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EffectKind::GitDiff { .. } | EffectKind::Difft { .. }
            )
        })
        .cloned()
        .collect();
    let requested = dumps.clone();
    for effect in dumps {
        let body = dump(&effect.kind);
        let (next, more) = feed(session, ok_child(effect.id, body));
        session = next;
        effects = more;
    }
    effects.extend(requested);
    (session, effects)
}

pub fn select_path(
    session: Session,
    path: &str,
    dump: impl Fn(&EffectKind) -> Vec<u8>,
) -> Session {
    let index = session
        .files_rows()
        .iter()
        .position(|r| r.path == path)
        .unwrap_or_else(|| panic!("no row {path}"));
    let (session, effects) = apply(session, Event::Mouse(MouseHit::Files { index }));
    finish_dumps(session, effects, dump).0
}

pub fn type_chars(mut session: Session, text: &str) -> Session {
    for c in text.chars() {
        let (next, _) = key(session, ch(c));
        session = next;
    }
    session
}
