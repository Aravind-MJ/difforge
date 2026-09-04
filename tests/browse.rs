//! Ticket 02 — browse all files, trees, and folders.

mod common;

use common::{ch, fail_child, feed, key, select_path, session_changed, session_with};
use difforge::{apply, EffectKind, Event, FilesMode, Key, MouseHit, RowKind};

#[test]
fn f_toggles_all_files_versus_changed_files() {
    let session = session_with(
        &[(" M", "src/a.rs")],
        &["src/a.rs", "src/b.rs", "README.md"],
        |_| b"dump\n".to_vec(),
    );
    assert_eq!(session.files_mode(), FilesMode::ChangedFlat);
    let (session, _) = key(session, ch('f'));
    assert_eq!(session.files_mode(), FilesMode::AllTree);
    let paths: Vec<_> = session.files_rows().iter().map(|r| r.path.as_str()).collect();
    assert!(paths.contains(&"README.md"));
    assert!(paths.contains(&"src"));
    let (session, _) = key(session, ch('f'));
    assert_eq!(session.files_mode(), FilesMode::ChangedFlat);
}

#[test]
fn backtick_and_t_toggle_flat_versus_tree_on_changed_files() {
    let session = session_changed(&[(" M", "src/a.rs"), (" M", "src/b.rs")]);
    let (session, _) = key(session, ch('`'));
    assert_eq!(session.files_mode(), FilesMode::ChangedTree);
    let (session, _) = key(session, ch('t'));
    assert_eq!(session.files_mode(), FilesMode::ChangedFlat);
}

#[test]
fn all_files_stays_a_tree_when_t_or_backtick_is_pressed() {
    let session = session_with(&[(" M", "a.rs")], &["a.rs", "b.rs"], |_| b"dump\n".to_vec());
    let (session, _) = key(session, ch('f'));
    assert_eq!(session.files_mode(), FilesMode::AllTree);
    let (session, _) = key(session, ch('t'));
    assert_eq!(session.files_mode(), FilesMode::AllTree);
    let (session, _) = key(session, ch('`'));
    assert_eq!(session.files_mode(), FilesMode::AllTree);
}

#[test]
fn tree_order_is_folders_first_then_files_az() {
    let session = session_changed(&[
        (" M", "z.rs"),
        (" M", "src/b.rs"),
        (" M", "src/a.rs"),
        (" M", "lib/c.rs"),
    ]);
    let (session, _) = key(session, ch('`'));
    let labels: Vec<_> = session
        .files_rows()
        .iter()
        .map(|r| (r.kind, r.path.as_str()))
        .collect();
    assert_eq!(
        labels,
        vec![
            (RowKind::Directory, "lib"),
            (RowKind::File, "lib/c.rs"),
            (RowKind::Directory, "src"),
            (RowKind::File, "src/a.rs"),
            (RowKind::File, "src/b.rs"),
            (RowKind::File, "z.rs"),
        ]
    );
}

#[test]
fn all_files_is_union_of_ls_files_and_porcelain_and_omits_ignored() {
    let session = session_with(
        &[("D ", "gone.rs"), (" M", "src/a.rs")],
        &["src/a.rs", "src/b.rs"],
        |_| b"dump\n".to_vec(),
    );
    let (session, _) = key(session, ch('f'));
    let files: Vec<_> = session
        .files_rows()
        .iter()
        .filter(|r| r.kind == RowKind::File)
        .map(|r| r.path.as_str())
        .collect();
    assert_eq!(files, vec!["src/a.rs", "src/b.rs", "gone.rs"]);
    assert!(!session
        .files_rows()
        .iter()
        .any(|r| r.path.contains("node_modules")));
}

#[test]
fn h_l_and_enter_fold_a_directory_and_enter_on_a_file_does_nothing() {
    let session = session_changed(&[(" M", "src/a.rs"), (" M", "src/b.rs")]);
    let (session, _) = key(session, ch('`'));
    let src = session
        .files_rows()
        .iter()
        .position(|r| r.path == "src")
        .unwrap();
    let (session, _) = apply(
        session,
        Event::Mouse(MouseHit::Files { index: src }),
    );
    let before = session.files_rows().len();
    let (session, _) = key(session, ch('h'));
    assert!(session.files_rows().len() < before);
    assert_eq!(
        session
            .files_rows()
            .iter()
            .find(|r| r.path == "src")
            .unwrap()
            .expanded,
        Some(false)
    );
    let (session, _) = key(session, ch('l'));
    assert_eq!(
        session
            .files_rows()
            .iter()
            .find(|r| r.path == "src")
            .unwrap()
            .expanded,
        Some(true)
    );
    let (session, _) = key(session, Key::Enter { ctrl: false });
    assert_eq!(
        session
            .files_rows()
            .iter()
            .find(|r| r.path == "src")
            .unwrap()
            .expanded,
        Some(false)
    );
    let (session, _) = key(session, ch('l'));
    let file = session
        .files_rows()
        .iter()
        .position(|r| r.path == "src/a.rs")
        .unwrap();
    let (session, _) = apply(session, Event::Mouse(MouseHit::Files { index: file }));
    let n = session.files_rows().len();
    let (session, _) = key(session, Key::Enter { ctrl: false });
    assert_eq!(session.files_rows().len(), n);
    assert_eq!(session.selected_path(), Some("src/a.rs"));
}

#[test]
fn long_names_truncate_to_the_files_panel_width() {
    let long = "abcdefghijklmnopqrstuvwxyz0123456789.rs";
    let session = session_changed(&[(" M", long)]);
    let display = &session.files_rows()[0].display;
    assert!(display.len() < format!(" M {long}").len());
    assert!(display.len() <= 28);
}

#[test]
fn focused_folder_in_changed_files_stacks_only_dirty_descendants() {
    let dump = |kind: &EffectKind| match kind {
        EffectKind::GitDiff { path, .. } => format!("DUMP {path}\n").into_bytes(),
        _ => b"dump\n".to_vec(),
    };
    let session = session_with(
        &[(" M", "src/a.rs"), (" M", "src/b.rs")],
        &["src/a.rs", "src/b.rs", "src/clean.rs"],
        dump,
    );
    let (session, _) = key(session, ch('`'));
    let session = select_path(session, "src", dump);
    let body = session.pane_body();
    assert!(body.contains("DUMP src/a.rs"), "{body}");
    assert!(body.contains("DUMP src/b.rs"), "{body}");
    assert!(!body.contains("clean.rs"), "{body}");
}

#[test]
fn unchanged_file_and_folder_in_all_files_show_no_changes() {
    let session = session_with(
        &[(" M", "src/a.rs")],
        &["src/a.rs", "src/clean.rs", "empty/kept.rs"],
        |_| b"dirty\n".to_vec(),
    );
    let (session, _) = key(session, ch('f'));
    let session = select_path(session, "src/clean.rs", |_| b"dirty\n".to_vec());
    assert_eq!(session.pane_body(), "src/clean.rs --- no changes");

    let session = select_path(session, "empty", |_| b"dirty\n".to_vec());
    assert!(
        session.pane_body().contains("empty/kept.rs --- no changes"),
        "{}",
        session.pane_body()
    );
}

#[test]
fn failed_dump_in_a_folder_stack_occupies_only_that_slot() {
    let session = session_changed(&[(" M", "src/a.rs"), (" M", "src/b.rs")]);
    let (session, _) = key(session, ch('`'));
    let src = session
        .files_rows()
        .iter()
        .position(|r| r.path == "src")
        .unwrap();
    let (session, effects) = apply(session, Event::Mouse(MouseHit::Files { index: src }));
    let diffs: Vec<_> = effects
        .iter()
        .filter(|e| matches!(e.kind, EffectKind::GitDiff { .. }))
        .cloned()
        .collect();
    assert_eq!(diffs.len(), 2);
    let (a, b) = (&diffs[0], &diffs[1]);
    let (session, _) = feed(session, fail_child(a.id, 2, b"boom\n", b""));
    let (session, _) = feed(session, ok_child_named(b.id, b"GOOD\n"));
    let body = session.pane_body();
    assert!(body.contains("boom"), "{body}");
    assert!(body.contains("GOOD"), "{body}");
}

fn ok_child_named(id: u64, stdout: &[u8]) -> difforge::ChildResult {
    common::ok_child(id, stdout.to_vec())
}

#[test]
fn rename_add_delete_and_unmerged_are_ordinary_paths() {
    let session = session_changed(&[
        ("R ", "renamed.rs"),
        ("A ", "added.rs"),
        ("D ", "deleted.rs"),
        ("UU", "conflict.rs"),
    ]);
    let paths: Vec<_> = session.files_rows().iter().map(|r| r.path.as_str()).collect();
    assert_eq!(
        paths,
        vec!["renamed.rs", "added.rs", "deleted.rs", "conflict.rs"]
    );
}

