//! Ticket 07 — refresh the files panel.

mod common;

use common::{ch, fail_child, feed, finish_dumps, finish_refresh, key, session_changed, take};
use difforge::{apply, EffectKind, Event, Key, Strip};

#[test]
fn r_reloads_porcelain_and_the_current_dump() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, effects) = key(session, ch('r'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitLsFiles)));
    let (session, effects) = finish_refresh(
        session,
        effects,
        &[(" M", "a.rs"), (" M", "b.rs")],
        &[],
        |_| b"new dump\n".to_vec(),
    );
    assert_eq!(session.files_rows().len(), 2);
    assert!(session.pane_body().contains("new dump"));
}

#[test]
fn tick_refreshes_when_idle_and_pauses_under_overlay_search_and_strip() {
    let session = session_changed(&[(" M", "a.rs")]);
    assert!(!session.poll_paused());
    let (_, effects) = apply(session, Event::Tick);
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));

    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    assert!(session.poll_paused());
    let (session, effects) = apply(session, Event::Tick);
    assert!(effects.is_empty());
    let (session, _) = key(session, Key::Esc);

    let session = session_changed(&[(" M", "a.rs")]);
    let (session, _) = key(session, ch('/'));
    assert!(session.poll_paused());
    let (session, effects) = apply(session, Event::Tick);
    assert!(effects.is_empty());
    let (session, effects) = key(session, Key::Esc);
    let session = finish_dumps(session, effects, |_| b"dump\n".to_vec()).0;
    assert!(!session.poll_paused());
    let (_, effects) = apply(session, Event::Tick);
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));

    let session = session_changed(&[]);
    let (session, _) = key(session, ch('c'));
    assert!(session.poll_paused());
    let (_, effects) = apply(session, Event::Tick);
    assert!(effects.is_empty());
}

#[test]
fn r_on_no_files_closes_the_strip_when_files_appear() {
    let session = session_changed(&[]);
    let (session, _) = key(session, ch('c'));
    assert_eq!(session.strip(), Some(&Strip::NoFilesCommitError));
    let (session, effects) = key(session, ch('r'));
    let (session, _) = finish_refresh(session, effects, &[(" M", "a.rs")], &[], |_| {
        b"dump\n".to_vec()
    });
    assert!(session.strip().is_none());
    assert!(!session.overlay_open());
}

#[test]
fn r_on_no_files_leaves_the_strip_when_porcelain_is_still_empty() {
    let session = session_changed(&[]);
    let (session, _) = key(session, ch('c'));
    let (session, effects) = key(session, ch('r'));
    let (session, _) = finish_refresh(session, effects, &[], &[], |_| b"".to_vec());
    assert_eq!(session.strip(), Some(&Strip::NoFilesCommitError));
}

#[test]
fn r_on_a_git_write_error_refreshes_and_leaves_the_strip() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, effects) = key(session, ch(' '));
    let write = take(&effects, |k| matches!(k, EffectKind::GitAdd { .. }));
    let (session, _) = feed(session, fail_child(write.id, 1, b"denied", b""));
    let (session, effects) = key(session, ch('r'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));
    let (session, _) = finish_refresh(session, effects, &[(" M", "a.rs")], &[], |_| {
        b"dump\n".to_vec()
    });
    match session.strip() {
        Some(Strip::GitWriteError { text }) => assert!(text.contains("denied")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn refresh_keeps_the_selected_path_or_walks_to_the_next_old_row() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs"), (" M", "c.rs")]);
    let (session, _) = key(session, ch('j'));
    assert_eq!(session.selected_path(), Some("b.rs"));
    let (session, effects) = key(session, ch('r'));
    let (session, _) = finish_refresh(
        session,
        effects,
        &[(" M", "a.rs"), (" M", "b.rs"), (" M", "c.rs")],
        &[],
        |_| b"dump\n".to_vec(),
    );
    assert_eq!(session.selected_path(), Some("b.rs"));

    let (session, effects) = key(session, ch('r'));
    let (session, _) = finish_refresh(
        session,
        effects,
        &[(" M", "a.rs"), (" M", "c.rs")],
        &[],
        |_| b"dump\n".to_vec(),
    );
    assert_eq!(session.selected_path(), Some("c.rs"));
}

#[test]
fn vanished_last_row_walks_to_the_previous_old_row() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs"), (" M", "c.rs")]);
    let (session, _) = key(session, ch('j'));
    let (session, _) = key(session, ch('j'));
    assert_eq!(session.selected_path(), Some("c.rs"));
    let (session, effects) = key(session, ch('r'));
    let (session, _) = finish_refresh(
        session,
        effects,
        &[(" M", "a.rs"), (" M", "b.rs")],
        &[],
        |_| b"dump\n".to_vec(),
    );
    assert_eq!(session.selected_path(), Some("b.rs"));
}

#[test]
fn empty_list_after_refresh_clears_the_selection() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, effects) = key(session, ch('r'));
    let (session, _) = finish_refresh(session, effects, &[], &[], |_| b"".to_vec());
    assert!(session.files_rows().is_empty());
    assert_eq!(session.selected_path(), None);
}

