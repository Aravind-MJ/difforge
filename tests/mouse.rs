//! Ticket 08 — point and type with the mouse.

mod common;

use common::{ch, fail_child, feed, key, session_changed, take, type_chars};
use difforge::{apply, Event, Key, MouseHit, Strip};

#[test]
fn click_selects_a_file_or_folder_without_folding() {
    let session = session_changed(&[(" M", "src/a.rs"), (" M", "src/b.rs")]);
    let (session, _) = key(session, ch('`'));
    let before = session.files_rows().len();
    let src = session
        .files_rows()
        .iter()
        .position(|r| r.path == "src")
        .unwrap();
    let (session, _) = apply(session, Event::Mouse(MouseHit::Files { index: src }));
    assert_eq!(session.selected_path(), Some("src"));
    assert_eq!(session.files_rows().len(), before);
    assert_eq!(
        session
            .files_rows()
            .iter()
            .find(|r| r.path == "src")
            .unwrap()
            .expanded,
        Some(true)
    );
}

#[test]
fn click_places_the_caret_in_search_and_commit_fields() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, _) = key(session, ch('/'));
    let session = type_chars(session, "abc");
    let (session, _) = apply(session, Event::Mouse(MouseHit::Search { col: 1 }));
    assert_eq!(session.caret(), 1);

    let session = session_changed(&[("M ", "a.rs")]);
    let (session, _) = key(session, ch('c'));
    let session = type_chars(session, "hello");
    let (session, _) = apply(session, Event::Mouse(MouseHit::Summary { col: 2 }));
    assert_eq!(session.commit_field(), Some(difforge::CommitField::Summary));
    assert_eq!(session.caret(), 2);
    let (session, _) = key(session, Key::Tab);
    let session = type_chars(session, "body");
    let (session, _) = apply(
        session,
        Event::Mouse(MouseHit::Description { col: 1, row: 0 }),
    );
    assert_eq!(
        session.commit_field(),
        Some(difforge::CommitField::Description)
    );
    assert_eq!(session.caret(), 1);
}

#[test]
fn clicks_are_inert_under_a_strip() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    let (session, effects) = key(session, ch(' '));
    let write = take(&effects, |k| matches!(k, difforge::EffectKind::GitAdd { .. }));
    let (session, _) = feed(session, fail_child(write.id, 1, b"nope", b""));
    assert!(matches!(session.strip(), Some(Strip::GitWriteError { .. })));
    let selected = session.selected_path().map(str::to_string);
    let (session, _) = apply(session, Event::Mouse(MouseHit::Files { index: 1 }));
    assert_eq!(session.selected_path(), selected.as_deref());
}
