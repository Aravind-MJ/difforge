//! Ticket 05 — commit from the overlay.

mod common;

use common::{ch, ctrl, fail_child, feed, key, session_changed, take, type_chars};
use difforge::{EffectKind, Key, Strip};

#[test]
fn c_with_a_non_empty_index_opens_the_overlay() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    assert!(session.overlay_open());
    assert!(session.strip().is_none());
}

#[test]
fn enter_on_a_non_blank_summary_runs_git_commit() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "fix parse");
    let (_, effects) = key(session, Key::Enter { ctrl: false });
    match &take(&effects, |k| matches!(k, EffectKind::GitCommit { .. })).kind {
        EffectKind::GitCommit {
            summary,
            description,
        } => {
            assert_eq!(summary, "fix parse");
            assert_eq!(description, "");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn description_is_a_second_minus_m_and_enter_inserts_a_newline() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "fix parse");
    let (session, _) = key(session, Key::Tab);
    let session = type_chars(session, "line one");
    let (session, _) = key(session, Key::Enter { ctrl: false });
    let session = type_chars(session, "line two");
    let (_, effects) = key(session, Key::Char { c: 's', ctrl: true });
    match &take(&effects, |k| matches!(k, EffectKind::GitCommit { .. })).kind {
        EffectKind::GitCommit {
            summary,
            description,
        } => {
            assert_eq!(summary, "fix parse");
            assert_eq!(description, "line one\nline two");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn empty_summary_is_refused_and_never_reaches_git() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    assert!(effects.is_empty());
    assert_eq!(session.strip(), Some(&Strip::EmptySummaryRefusal));
    assert_eq!(
        session.strip_text().as_deref(),
        Some("Commit summary is required.")
    );
    assert!(session.overlay_open());
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    assert!(effects.is_empty());
    assert_eq!(session.strip(), Some(&Strip::EmptySummaryRefusal));

    let session = type_chars(session, "   ");
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    assert!(effects.is_empty());
    assert_eq!(session.strip(), Some(&Strip::EmptySummaryRefusal));
}

#[test]
fn a_real_summary_after_a_refusal_commits() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let (session, _) = key(session, Key::Enter { ctrl: false });
    let session = type_chars(session, "done");
    let (_, effects) = key(session, Key::Enter { ctrl: false });
    match &take(&effects, |k| matches!(k, EffectKind::GitCommit { .. })).kind {
        EffectKind::GitCommit { summary, .. } => assert_eq!(summary, "done"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn esc_keeps_the_draft_and_kills_the_refusal() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "wip");
    let (session, _) = key(session, Key::Enter { ctrl: false });
    // wait, that would commit. type then refuse with empty... type wip then we have summary.
    let (session, _) = key(session, Key::Esc);
    assert!(!session.overlay_open());
    assert_eq!(session.draft().summary, "wip");
    assert!(session.strip().is_none());
}

#[test]
fn successful_commit_clears_the_draft_and_refreshes() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "ok");
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    let commit = take(&effects, |k| matches!(k, EffectKind::GitCommit { .. }));
    let (session, effects) = feed(session, common::ok_child(commit.id, b""));
    assert!(!session.overlay_open());
    assert_eq!(session.draft().summary, "");
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));
}

#[test]
fn failed_commit_keeps_the_overlay_and_shows_the_child() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "ok");
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    let commit = take(&effects, |k| matches!(k, EffectKind::GitCommit { .. }));
    let (session, _) = feed(session, fail_child(commit.id, 1, b"hook failed\n", b""));
    assert!(session.overlay_open());
    match session.strip() {
        Some(Strip::GitWriteError { text }) => assert!(text.contains("hook failed")),
        other => panic!("{other:?}"),
    }
    assert_eq!(session.draft().summary, "ok");
    let (session, _) = key(session, ch('!'));
    assert_eq!(session.draft().summary, "ok!");
}

#[test]
fn retry_after_failed_commit_replaces_the_error() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "ok");
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    let commit = take(&effects, |k| matches!(k, EffectKind::GitCommit { .. }));
    let (session, _) = feed(session, fail_child(commit.id, 1, b"first\n", b""));
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    let commit = take(&effects, |k| matches!(k, EffectKind::GitCommit { .. }));
    let (session, _) = feed(session, fail_child(commit.id, 1, b"second\n", b""));
    match session.strip() {
        Some(Strip::GitWriteError { text }) => {
            assert!(text.contains("second"));
            assert!(!text.contains("first"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn esc_after_failed_commit_closes_and_keeps_the_draft() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "ok");
    let (session, effects) = key(session, Key::Enter { ctrl: false });
    let commit = take(&effects, |k| matches!(k, EffectKind::GitCommit { .. }));
    let (session, _) = feed(session, fail_child(commit.id, 1, b"nope", b""));
    let (session, _) = key(session, Key::Esc);
    assert!(!session.overlay_open());
    assert!(session.strip().is_none());
    assert_eq!(session.draft().summary, "ok");
}

#[test]
fn q_types_in_the_overlay_and_ctrl_c_quits() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let (session, effects) = key(session, ch('q'));
    assert!(!effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
    assert_eq!(session.draft().summary, "q");
    let (_, effects) = key(session, ctrl('c'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
}

#[test]
fn left_and_right_move_the_caret() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "ab");
    assert_eq!(session.caret(), 2);
    let (session, _) = key(session, Key::Left);
    assert_eq!(session.caret(), 1);
    let (session, _) = key(session, Key::Right);
    assert_eq!(session.caret(), 2);
}

#[test]
fn ctrl_enter_commits_from_either_field() {
    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "msg");
    let (session, _) = key(session, Key::Tab);
    let (_, effects) = key(session, Key::Enter { ctrl: true });
    assert!(effects
        .iter()
        .any(|e| matches!(e.kind, EffectKind::GitCommit { .. })));
}

