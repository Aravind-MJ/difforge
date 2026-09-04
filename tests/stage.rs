//! Ticket 04 — stage and unstage a whole file.

mod common;

use common::{
    ch, fail_child, feed, finish_dumps, key, ok_child, session_changed, session_with, take, COLS,
    ROWS,
};
use difforge::{apply, EffectKind, Event, Key, MouseHit, Session, Strip};

#[test]
fn space_on_unstaged_runs_git_add() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (_, effects) = key(session, ch(' '));
    match &take(&effects, |k| matches!(k, EffectKind::GitAdd { .. })).kind {
        EffectKind::GitAdd { path } => assert_eq!(path, "a.rs"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn space_on_fully_staged_tracked_runs_git_reset() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (_, effects) = key(session, ch(' '));
    match &take(&effects, |k| matches!(k, EffectKind::GitReset { .. })).kind {
        EffectKind::GitReset { path } => assert_eq!(path, "a.rs"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn space_on_staged_new_runs_git_rm_cached() {
    let session = session_changed(&[("A ", "new.rs")]);
    let (_, effects) = key(session, ch(' '));
    match &take(&effects, |k| matches!(k, EffectKind::GitRmCached { .. })).kind {
        EffectKind::GitRmCached { path } => assert_eq!(path, "new.rs"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn space_on_untracked_runs_git_add() {
    let session = session_changed(&[("??", "new.rs")]);
    let (_, effects) = key(session, ch(' '));
    assert!(matches!(
        take(&effects, |k| matches!(k, EffectKind::GitAdd { .. })).kind,
        EffectKind::GitAdd { .. }
    ));
}

#[test]
fn space_on_renamed_and_dirty_adds_the_destination() {
    // git status --porcelain -z: XY dest\0src\0. The work-tree file is dest.
    let porcelain = b"RM dest.rs\0src.rs\0";
    let (session, effects) = Session::boot(COLS, ROWS);
    let status = take(&effects, |k| matches!(k, EffectKind::GitPorcelain)).id;
    let ls = take(&effects, |k| matches!(k, EffectKind::GitLsFiles)).id;
    let (session, _effects) = feed(session, ok_child(status, porcelain.to_vec()));
    let (session, effects) = feed(session, ok_child(ls, common::ls_files_z(&["dest.rs"])));
    let session = finish_dumps(session, effects, |_| b"dump\n".to_vec()).0;
    assert_eq!(session.files_rows()[0].path, "dest.rs");
    let (_, effects) = key(session, ch(' '));
    match &take(&effects, |k| matches!(k, EffectKind::GitAdd { .. })).kind {
        EffectKind::GitAdd { path } => assert_eq!(path, "dest.rs"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn space_on_both_staged_and_dirty_adds_first() {
    let session = session_changed(&[("MM", "a.rs")]);
    let (_, effects) = key(session, ch(' '));
    assert!(matches!(
        take(&effects, |k| matches!(k, EffectKind::GitAdd { .. })).kind,
        EffectKind::GitAdd { .. }
    ));
}

#[test]
fn space_on_a_directory_or_clean_path_does_nothing() {
    let session = session_changed(&[(" M", "src/a.rs")]);
    let (session, _) = key(session, ch('`'));
    let src = session
        .files_rows()
        .iter()
        .position(|r| r.path == "src")
        .unwrap();
    let (session, _) = apply(session, Event::Mouse(MouseHit::Files { index: src }));
    let (_, effects) = key(session, ch(' '));
    assert!(!effects.iter().any(|e| matches!(
        e.kind,
        EffectKind::GitAdd { .. } | EffectKind::GitReset { .. } | EffectKind::GitRmCached { .. }
    )));

    let session = session_with(&[(" M", "src/a.rs")], &["src/a.rs", "clean.rs"], |_| {
        b"dump\n".to_vec()
    });
    let (session, _) = key(session, ch('f'));
    let clean = session
        .files_rows()
        .iter()
        .position(|r| r.path == "clean.rs")
        .unwrap();
    let (session, _) = apply(session, Event::Mouse(MouseHit::Files { index: clean }));
    let (_, effects) = key(session, ch(' '));
    assert!(effects.is_empty());
}

#[test]
fn successful_space_refreshes_immediately() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, effects) = key(session, ch(' '));
    let write = take(&effects, |k| matches!(k, EffectKind::GitAdd { .. }));
    let (session, effects) = feed(session, common::ok_child(write.id, b""));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));
}

#[test]
fn failed_space_keeps_the_list_and_opens_a_git_write_error() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    let (session, effects) = key(session, ch(' '));
    let write = take(&effects, |k| matches!(k, EffectKind::GitAdd { .. }));
    let (session, effects) = feed(session, fail_child(write.id, 1, b"denied\n", b""));
    assert!(effects.is_empty());
    assert_eq!(session.files_rows().len(), 2);
    match session.strip() {
        Some(Strip::GitWriteError { text }) => assert!(text.contains("denied")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn empty_failed_write_shows_git_failed_exit() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, effects) = key(session, ch(' '));
    let write = take(&effects, |k| matches!(k, EffectKind::GitAdd { .. }));
    let (session, _) = feed(session, fail_child(write.id, 128, b"", b""));
    match session.strip() {
        Some(Strip::GitWriteError { text }) => assert_eq!(text, "git failed (exit 128)"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn esc_or_enter_dismisses_a_space_write_error_and_keys_are_inert() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    let (session, effects) = key(session, ch(' '));
    let write = take(&effects, |k| matches!(k, EffectKind::GitAdd { .. }));
    let (session, _) = feed(session, fail_child(write.id, 1, b"nope", b""));
    let selected = session.selected_path().map(str::to_string);
    let (session, effects) = key(session, ch('j'));
    assert!(effects.is_empty());
    assert_eq!(session.selected_path(), selected.as_deref());
    let (session, _) = key(session, Key::PageDown);
    assert!(session.diff_scroll() > 0);
    let (session, effects) = key(session, ch('q'));
    assert!(!effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
    let (session, _) = key(session, Key::Esc);
    assert!(session.strip().is_none());
    let (session, effects) = key(session, ch('q'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
}
