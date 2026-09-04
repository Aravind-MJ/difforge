//! Ticket 01 — launch with changed files and a path's diff.

mod common;

use common::{
    ch, ctrl, fail_child, feed, finish_refresh, key, ok_child, pane_width, session_changed,
    session_with, take, COLS, ROWS,
};
use difforge::{apply, EffectKind, Event, FilesMode, Focus, Key, RowColor, Session};

#[test]
fn boot_requests_porcelain_and_ls_files_and_stays_on_the_session() {
    let (session, effects) = Session::boot(COLS, ROWS);
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitPorcelain)));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::GitLsFiles)));
    assert!(!effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
    assert_eq!(session.files_mode(), FilesMode::ChangedFlat);
    assert_eq!(session.focus(), Focus::Files);
}

#[test]
fn changed_files_are_a_flat_list_in_porcelain_order_with_xy_and_full_path() {
    let session = session_changed(&[
        (" M", "src/b.rs"),
        ("M ", "src/a.rs"),
        ("??", "scratch.txt"),
    ]);
    let paths: Vec<_> = session
        .files_rows()
        .iter()
        .map(|r| (r.xy.as_str(), r.path.as_str()))
        .collect();
    assert_eq!(
        paths,
        vec![
            (" M", "src/b.rs"),
            ("M ", "src/a.rs"),
            ("??", "scratch.txt"),
        ]
    );
    assert!(session.files_rows().iter().all(|r| r.depth == 0));
}

#[test]
fn both_staged_and_dirty_is_one_row() {
    let session = session_changed(&[("MM", "src/both.rs")]);
    assert_eq!(session.files_rows().len(), 1);
    assert_eq!(session.files_rows()[0].path, "src/both.rs");
    assert_eq!(session.files_rows()[0].color, RowColor::Both);
}

#[test]
fn row_color_follows_staged_both_and_unstaged() {
    let session = session_changed(&[
        ("M ", "staged.rs"),
        ("MM", "both.rs"),
        (" M", "unstaged.rs"),
        ("??", "new.txt"),
    ]);
    let colors: Vec<_> = session.files_rows().iter().map(|r| r.color).collect();
    assert_eq!(
        colors,
        vec![
            RowColor::Staged,
            RowColor::Both,
            RowColor::Default,
            RowColor::Default,
        ]
    );
}

#[test]
fn tab_moves_focus_between_files_panel_and_diff_pane() {
    let session = session_changed(&[(" M", "a.rs")]);
    assert_eq!(session.focus(), Focus::Files);
    let (session, _) = key(session, Key::Tab);
    assert_eq!(session.focus(), Focus::Diff);
    let (session, _) = key(session, Key::Tab);
    assert_eq!(session.focus(), Focus::Files);
}

#[test]
fn j_k_move_files_selection_and_reset_diff_scroll() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    assert_eq!(session.selected_path(), Some("a.rs"));
    let (session, _) = key(session, Key::PageDown);
    assert!(session.diff_scroll() > 0, "page down should scroll the dump");
    let (session, _) = key(session, ch('j'));
    assert_eq!(session.selected_path(), Some("b.rs"));
    assert_eq!(session.diff_scroll(), 0);
    let (session, _) = key(session, ch('k'));
    assert_eq!(session.selected_path(), Some("a.rs"));
}

#[test]
fn j_k_scroll_the_dump_when_the_diff_pane_is_focused() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    let (session, _) = key(session, Key::Tab);
    assert_eq!(session.focus(), Focus::Diff);
    let (session, _) = key(session, ch('j'));
    assert_eq!(session.selected_path(), Some("a.rs"));
    assert!(session.diff_scroll() > 0);
    let (session, _) = key(session, ch('k'));
    assert_eq!(session.diff_scroll(), 0);
}

#[test]
fn page_up_and_page_down_always_scroll_the_diff() {
    let session = session_changed(&[(" M", "a.rs")]);
    assert_eq!(session.focus(), Focus::Files);
    let (session, _) = key(session, Key::PageDown);
    assert!(session.diff_scroll() > 0);
    let (session, _) = key(session, Key::PageUp);
    assert_eq!(session.diff_scroll(), 0);
}

#[test]
fn dump_scroll_stops_when_the_last_line_is_on_screen() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (mut session, _) = key(session, Key::Tab);
    let mut last = session.diff_scroll();
    for _ in 0..200 {
        let (next, _) = key(session, ch('j'));
        if next.diff_scroll() == last {
            session = next;
            break;
        }
        last = next.diff_scroll();
        session = next;
    }
    let (session, _) = key(session, ch('j'));
    assert_eq!(session.diff_scroll(), last);
    let lines = session.pane_body().lines().count() as u16;
    let visible = ROWS.saturating_sub(1);
    assert_eq!(
        last,
        lines.saturating_sub(visible),
        "scroll {last} should leave the last line in a {visible}-row pane of {lines} lines"
    );
}

#[test]
fn rest_footer_names_what_the_keys_do() {
    let session = session_changed(&[(" M", "a.rs")]);
    let footer = session.footer_text();
    for needle in [
        "j/k move",
        "h/l fold",
        "f all/changed",
        "` tree/flat",
        "/ search",
        "tab focus",
        "space stage",
        "c commit",
        "r refresh",
        "q quit",
    ] {
        assert!(footer.contains(needle), "missing {needle:?} in {footer:?}");
    }
    let (session, _) = key(session, Key::Tab);
    assert!(
        session.footer_text().contains("j/k scroll"),
        "diff focus should say scroll, got {:?}",
        session.footer_text()
    );
}

#[test]
fn q_at_rest_and_ctrl_c_ask_to_quit() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (_, effects) = key(session, ch('q'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));

    let session = session_changed(&[(" M", "a.rs")]);
    let (_, effects) = key(session, ctrl('c'));
    assert!(effects.iter().any(|e| matches!(e.kind, EffectKind::Quit)));
}

#[test]
fn only_unstaged_requests_work_tree_vs_index() {
    let (session, effects) = Session::boot(COLS, ROWS);
    let (_, effects) = finish_refresh(session, effects, &[(" M", "a.rs")], &[], |_| Vec::new());
    let diff = take(&effects, |k| matches!(k, EffectKind::GitDiff { .. }));
    match &diff.kind {
        EffectKind::GitDiff {
            path,
            cached,
            width,
        } => {
            assert_eq!(path, "a.rs");
            assert!(!*cached);
            assert_eq!(*width, pane_width());
        }
        other => panic!("expected GitDiff, got {other:?}"),
    }
}

#[test]
fn only_staged_requests_index_vs_head() {
    let (session, effects) = Session::boot(COLS, ROWS);
    let (_, effects) = finish_refresh(session, effects, &[("M ", "a.rs")], &[], |_| Vec::new());
    let diff = take(&effects, |k| matches!(k, EffectKind::GitDiff { .. }));
    match &diff.kind {
        EffectKind::GitDiff { cached, .. } => assert!(*cached),
        other => panic!("expected GitDiff, got {other:?}"),
    }
}

#[test]
fn untracked_requests_difft_against_dev_null() {
    let (session, effects) = Session::boot(COLS, ROWS);
    let (_, effects) = finish_refresh(session, effects, &[("??", "new.rs")], &[], |_| Vec::new());
    let dump = take(&effects, |k| matches!(k, EffectKind::Difft { .. }));
    match &dump.kind {
        EffectKind::Difft { path, width } => {
            assert_eq!(path, "new.rs");
            assert_eq!(*width, pane_width());
        }
        other => panic!("expected Difft, got {other:?}"),
    }
}

#[test]
fn both_staged_and_dirty_requests_unstaged_then_staged() {
    let (session, effects) = Session::boot(COLS, ROWS);
    let (_, effects) = finish_refresh(session, effects, &[("MM", "a.rs")], &[], |_| Vec::new());
    let diffs: Vec<_> = effects
        .iter()
        .filter_map(|e| match &e.kind {
            EffectKind::GitDiff { cached, .. } => Some(*cached),
            _ => None,
        })
        .collect();
    assert_eq!(diffs, vec![false, true]);
}

#[test]
fn pane_shows_successful_dump_text() {
    let session = session_with(&[(" M", "a.rs")], &[], |_| b"hello dump\n".to_vec());
    assert!(session.pane_body().contains("hello dump"));
}

#[test]
fn failed_git_diff_replaces_the_pane_with_stderr_then_stdout() {
    let (session, effects) = Session::boot(COLS, ROWS);
    let status = take(&effects, |k| matches!(k, EffectKind::GitPorcelain)).id;
    let ls = take(&effects, |k| matches!(k, EffectKind::GitLsFiles)).id;
    let (session, effects) = feed(session, ok_child(status, common::porcelain_z(&[(" M", "a.rs")])));
    let (session, effects) = feed(session, ok_child(ls, Vec::new()));
    let dump = take(&effects, |k| matches!(k, EffectKind::GitDiff { .. }));
    let (session, _) = feed(
        session,
        fail_child(dump.id, 2, b"fatal: died\n", b"leftover\n"),
    );
    assert_eq!(session.pane_body(), "fatal: died\nleftover");
}

#[test]
fn empty_failed_child_shows_named_exit() {
    let (session, effects) = Session::boot(COLS, ROWS);
    let status = take(&effects, |k| matches!(k, EffectKind::GitPorcelain)).id;
    let ls = take(&effects, |k| matches!(k, EffectKind::GitLsFiles)).id;
    let (session, effects) = feed(session, ok_child(status, common::porcelain_z(&[(" M", "a.rs")])));
    let (session, effects) = feed(session, ok_child(ls, Vec::new()));
    let dump = take(&effects, |k| matches!(k, EffectKind::GitDiff { .. }));
    let (session, _) = feed(session, fail_child(dump.id, 2, b"", b""));
    assert_eq!(session.pane_body(), "git failed (exit 2)");

    let (session, effects) = Session::boot(COLS, ROWS);
    let status = take(&effects, |k| matches!(k, EffectKind::GitPorcelain)).id;
    let ls = take(&effects, |k| matches!(k, EffectKind::GitLsFiles)).id;
    let (session, effects) = feed(session, ok_child(status, common::porcelain_z(&[("??", "n.rs")])));
    let (session, effects) = feed(session, ok_child(ls, Vec::new()));
    let dump = take(&effects, |k| matches!(k, EffectKind::Difft { .. }));
    let (session, _) = feed(session, fail_child(dump.id, 1, b"", b""));
    assert_eq!(session.pane_body(), "difft failed (exit 1)");
}

#[test]
fn changing_the_selected_path_requests_a_new_dump() {
    let session = session_changed(&[(" M", "a.rs"), (" M", "b.rs")]);
    let (session, effects) = key(session, ch('j'));
    let diff = take(&effects, |k| matches!(k, EffectKind::GitDiff { .. }));
    match &diff.kind {
        EffectKind::GitDiff { path, .. } => assert_eq!(path, "b.rs"),
        other => panic!("expected GitDiff, got {other:?}"),
    }
    assert!(session.dumps_pending());
    assert!(
        session.pane_body().contains("line 0"),
        "keep the last dump until the new one arrives, got {:?}",
        session.pane_body()
    );
}

#[test]
fn resize_that_changes_inner_width_re_runs_difft() {
    let session = session_changed(&[(" M", "a.rs")]);
    let (session, effects) = apply(session, Event::Resize { cols: 100, rows: ROWS });
    let diff = take(&effects, |k| matches!(k, EffectKind::GitDiff { .. }));
    match &diff.kind {
        EffectKind::GitDiff { width, .. } => assert_eq!(*width, 100 - 28),
        other => panic!("expected GitDiff, got {other:?}"),
    }
    let (session, effects) = apply(session, Event::Resize { cols: 100, rows: 30 });
    assert!(!effects.iter().any(|e| matches!(e.kind, EffectKind::GitDiff { .. })));
    let _ = session;
}

#[test]
fn short_side_by_side_line_pads_only_the_left_column() {
    // Two line-number spans ("  1 " twice) with a short left column.
    let ansi = b"\x1b[32m  1 \x1b[0mhi\x1b[32m  1 \x1b[0mthere\n";
    let session = session_with(&[(" M", "a.rs")], &[], |_| ansi.to_vec());
    let body = session.pane_body();
    let target = ((pane_width().saturating_sub(1) / 2) + 1) as usize;
    let rhs = body.find("there").expect("right column");
    assert!(
        rhs >= target,
        "right column starts at {rhs}, want >= {target} in {body:?}"
    );
    assert!(body.contains("hi"));
}

#[test]
fn wrap_continuation_is_not_padded_as_a_short_column() {
    // difftastic wrap marker is "... " then the next side's line number.
    // Treating "..." as a line number inserts a half-pane gap into the wrap.
    let ansi = b"\x1b[2m... \x1b[0m\x1b[2m340 \x1b[0m        List::new(items)\n";
    let session = session_with(&[(" M", "a.rs")], &[], |_| ansi.to_vec());
    let body = session.pane_body();
    assert!(
        body.contains("... 340 "),
        "wrap marker must stay next to the following line number, got {body:?}"
    );
}

#[test]
fn dump_containing_nul_is_a_binary_one_liner() {
    // difft treats some large binaries (e.g. a file of NULs) as Text and
    // dumps the bytes. Keeping that in the pane freezes every draw.
    let mut ansi = b"\x1b[1mblob.bin\x1b[0m --- Text\n\x1b[92m1 \x1b[0m".to_vec();
    ansi.extend(std::iter::repeat(0u8).take(200_000));
    ansi.extend(b"\n");
    let session = session_with(&[("??", "blob.bin")], &[], |_| ansi.clone());
    let body = session.pane_body();
    assert!(
        body.contains("Binary"),
        "NUL dump must become the binary one-liner, got {body:?}"
    );
    assert!(
        body.len() < 80,
        "must not keep the NUL payload, got {} bytes: {body:?}",
        body.len()
    );
}

#[test]
fn empty_left_column_is_not_padded_as_a_short_column() {
    // Two-column wrap groups start with both line numbers and no left content.
    // Padding that first line (and not the "... " continuations) shifts the wrap.
    let ansi = b"\x1b[2m339 \x1b[0m\x1b[2m339 \x1b[0m    frame.render_stateful_widget(\n";
    let session = session_with(&[(" M", "a.rs")], &[], |_| ansi.to_vec());
    let body = session.pane_body();
    assert!(
        body.contains("339 339 "),
        "adjacent line numbers must stay together when the left column is empty, got {body:?}"
    );
}
