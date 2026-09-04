//! Ticket 03 — filter the files panel.

mod common;

use common::{ch, key, select_path, session_changed, session_with, type_chars};
use difforge::{Key, Strip};

#[test]
fn slash_filters_live_by_a_case_insensitive_path_substring() {
    let session = session_changed(&[(" M", "src/Foo.rs"), (" M", "lib/bar.rs")]);
    let (session, _) = key(session, ch('/'));
    assert!(session.search_open());
    let session = type_chars(session, "FOO");
    assert_eq!(session.search_query(), "FOO");
    let paths: Vec<_> = session.files_rows().iter().map(|r| r.path.as_str()).collect();
    assert_eq!(paths, vec!["src/Foo.rs"]);
}

#[test]
fn enter_keeps_the_filter_and_esc_clears_it() {
    let session = session_changed(&[(" M", "src/Foo.rs"), (" M", "lib/bar.rs")]);
    let (session, _) = key(session, ch('/'));
    let session = type_chars(session, "foo");
    let (session, _) = key(session, Key::Enter { ctrl: false });
    assert!(!session.search_open());
    assert_eq!(session.search_query(), "foo");
    assert_eq!(session.files_rows().len(), 1);

    let (session, _) = key(session, ch('/'));
    let (session, _) = key(session, Key::Esc);
    assert!(!session.search_open());
    assert_eq!(session.search_query(), "");
    assert_eq!(session.files_rows().len(), 2);
}

#[test]
fn q_types_the_letter_while_file_search_is_open() {
    let session = session_changed(&[(" M", "src/q.rs"), (" M", "src/a.rs")]);
    let (session, _) = key(session, ch('/'));
    let (session, effects) = key(session, ch('q'));
    assert!(!effects.iter().any(|e| matches!(e.kind, difforge::EffectKind::Quit)));
    assert_eq!(session.search_query(), "q");
    assert_eq!(session.files_rows().len(), 1);
    assert_eq!(session.files_rows()[0].path, "src/q.rs");
}

#[test]
fn focused_folder_stack_follows_the_filter() {
    let dump = |kind: &difforge::EffectKind| match kind {
        difforge::EffectKind::GitDiff { path, .. } => format!("DUMP {path}\n").into_bytes(),
        _ => b"dump\n".to_vec(),
    };
    let session = session_changed(&[(" M", "src/a.rs"), (" M", "src/keep.rs")]);
    let (session, _) = key(session, ch('`'));
    let (session, _) = key(session, ch('/'));
    let session = type_chars(session, "keep");
    let (session, _) = key(session, Key::Enter { ctrl: false });
    let session = select_path(session, "src", dump);
    let body = session.pane_body();
    assert!(body.contains("DUMP src/keep.rs"), "{body}");
    assert!(!body.contains("DUMP src/a.rs"), "{body}");
}

#[test]
fn a_filter_that_matches_nothing_empties_the_panel_and_clears_selection() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, _) = key(session, ch('/'));
    let session = type_chars(session, "zzz");
    assert!(session.files_rows().is_empty());
    assert_eq!(session.selected_path(), None);
}

#[test]
fn c_consults_porcelain_not_the_filtered_rows() {
    let session = session_changed(&[("M ", "keep.rs"), (" M", "other.rs")]);
    let (session, _) = key(session, ch('/'));
    let session = type_chars(session, "zzz");
    assert!(session.files_rows().is_empty());
    let (session, _) = key(session, Key::Enter { ctrl: false });
    let (session, _) = key(session, ch('c'));
    assert!(session.overlay_open());
    assert_ne!(session.strip(), Some(&Strip::NoFilesCommitError));
}
