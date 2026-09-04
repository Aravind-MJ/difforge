//! Ticket 06 — gate commit when nothing is staged.

mod common;

use common::{ch, fail_child, feed, key, session_changed, take};
use difforge::{EffectKind, Key, Strip};

#[test]
fn c_with_empty_porcelain_opens_the_no_files_commit_error() {
    let session = session_changed(&[]);
    let (session, _) = key(session, ch('c'));
    assert_eq!(session.strip(), Some(&Strip::NoFilesCommitError));
    assert_eq!(session.strip_text().as_deref(), Some("No files to commit."));
    assert!(!session.overlay_open());
}

#[test]
fn esc_or_enter_dismisses_the_no_files_commit_error() {
    let session = session_changed(&[]);
    let (session, _) = key(session, ch('c'));
    let (session, _) = key(session, Key::Esc);
    assert!(session.strip().is_none());
    let (session, _) = key(session, ch('c'));
    let (session, _) = key(session, Key::Enter { ctrl: false });
    assert!(session.strip().is_none());
}

#[test]
fn c_with_files_and_an_empty_index_opens_the_nothing_staged_confirm() {
    let session = session_changed(&[(" M", "a.rs"), ("??", "b.rs")]);
    let (session, _) = key(session, ch('c'));
    assert_eq!(session.strip(), Some(&Strip::NothingStagedConfirm));
    assert_eq!(
        session.strip_text().as_deref(),
        Some("Nothing staged. Stage all changes and commit?")
    );
    assert!(!session.overlay_open());
}

#[test]
fn y_or_enter_on_the_confirm_runs_git_add_all_then_opens_the_overlay() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let (session, effects) = key(session, ch('y'));
    let add = take(&effects, |k| matches!(k, EffectKind::GitAddAll));
    let (session, effects) = feed(session, common::ok_child(add.id, b""));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));
    let (session, _) = common::finish_refresh(
        session,
        effects,
        &[("M ", "a.rs")],
        &[],
        |_| b"dump\n".to_vec(),
    );
    assert!(session.overlay_open());
    assert!(session.strip().is_none());
}

#[test]
fn n_or_esc_closes_the_confirm_and_leaves_the_index() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let (session, effects) = key(session, ch('n'));
    assert!(effects.is_empty());
    assert!(session.strip().is_none());
    assert!(!session.overlay_open());
}

#[test]
fn failed_add_all_replaces_the_confirm_and_stays_off_the_overlay() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    let add = take(&effects, |k| matches!(k, EffectKind::GitAddAll));
    let (session, _) = feed(session, fail_child(add.id, 1, b"cannot add\n", b""));
    assert!(!session.overlay_open());
    match session.strip() {
        Some(Strip::GitWriteError { text }) => assert!(text.contains("cannot add")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn confirm_and_no_files_strips_are_inert() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    let (session, _) = key(session, ch('c'));
    let selected = session.selected_path().map(str::to_string);
    let (session, effects) = key(session, ch('j'));
    assert_eq!(session.selected_path(), selected.as_deref());
    let (session, effects2) = key(session, ch('q'));
    assert!(!effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
    assert!(!effects2.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
    let (_, effects) = key(session, common::ctrl('c'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
}
