//! Porcelain parse and files-panel rows.

use std::collections::HashSet;

use unicode_width::UnicodeWidthStr;

use crate::event::{FilesMode, FilesRow, RowColor, RowKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PorcelainEntry {
    pub xy: [char; 2],
    pub path: String,
}

pub(crate) fn parse_porcelain(bytes: &[u8]) -> Vec<PorcelainEntry> {
    let mut entries = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        if rest.len() < 3 {
            break;
        }
        let x = rest[0] as char;
        let y = rest[1] as char;
        rest = &rest[2..];
        if rest.first() == Some(&b' ') {
            rest = &rest[1..];
        }
        let (first, after) = split_nul(rest);
        rest = after;
        let path = if matches!(x, 'R' | 'C') && !looks_like_entry(rest) {
            // porcelain -z: destination NUL source. The work-tree file is dest.
            let (_src, after) = split_nul(rest);
            rest = after;
            String::from_utf8_lossy(&first).into_owned()
        } else {
            String::from_utf8_lossy(&first).into_owned()
        };
        if !path.is_empty() {
            entries.push(PorcelainEntry { xy: [x, y], path });
        }
    }
    entries
}

pub(crate) fn parse_ls_files(bytes: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (path, after) = split_nul(rest);
        rest = after;
        if !path.is_empty() {
            paths.push(String::from_utf8_lossy(&path).into_owned());
        }
    }
    paths
}

fn looks_like_entry(bytes: &[u8]) -> bool {
    bytes.len() >= 3 && bytes[2] == b' '
}

fn split_nul(bytes: &[u8]) -> (&[u8], &[u8]) {
    match bytes.iter().position(|&b| b == 0) {
        Some(i) => (&bytes[..i], &bytes[i + 1..]),
        None => (bytes, &[]),
    }
}

pub(crate) fn row_color(xy: [char; 2]) -> RowColor {
    let staged = xy[0] != ' ' && xy[0] != '?';
    if staged && xy[1] != ' ' {
        RowColor::Both
    } else if staged {
        RowColor::Staged
    } else {
        RowColor::Default
    }
}

pub(crate) fn has_unstaged(xy: [char; 2]) -> bool {
    xy == ['?', '?'] || xy[1] != ' '
}

pub(crate) fn is_untracked(xy: [char; 2]) -> bool {
    xy == ['?', '?']
}

pub(crate) fn is_in_head(xy: [char; 2]) -> bool {
    xy[0] != 'A' && xy != ['?', '?']
}

pub(crate) fn has_staged(xy: [char; 2]) -> bool {
    xy[0] != ' ' && xy[0] != '?'
}

pub(crate) fn xy_of<'a>(porcelain: &'a [PorcelainEntry], path: &str) -> Option<[char; 2]> {
    porcelain.iter().find(|e| e.path == path).map(|e| e.xy)
}

#[derive(Clone, Debug)]
struct Node {
    name: String,
    path: String,
    dir: bool,
    xy: Option<[char; 2]>,
    children: Vec<Node>,
}

pub(crate) fn visible_rows(
    porcelain: &[PorcelainEntry],
    ls_files: &[String],
    mode: FilesMode,
    collapsed: &HashSet<String>,
    filter: Option<&str>,
    width: usize,
) -> Vec<FilesRow> {
    match mode {
        FilesMode::ChangedFlat => {
            porcelain
                .iter()
                .filter(|e| matches_filter(&e.path, filter))
                .map(|e| {
                    let display = truncate(&format!("{} {}", chars(&e.xy), e.path), width);
                    FilesRow {
                        path: e.path.clone(),
                        display,
                        xy: chars(&e.xy),
                        kind: RowKind::File,
                        color: row_color(e.xy),
                        depth: 0,
                        expanded: None,
                    }
                })
                .collect()
        }
        FilesMode::ChangedTree => {
            let paths: Vec<(String, Option<[char; 2]>)> = porcelain
                .iter()
                .map(|e| (e.path.clone(), Some(e.xy)))
                .collect();
            flatten_tree(&build_tree(&paths), collapsed, filter, width)
        }
        FilesMode::AllTree => {
            let mut seen = HashSet::new();
            let mut paths = Vec::new();
            for path in ls_files {
                if seen.insert(path.clone()) {
                    let xy = xy_of(porcelain, path);
                    paths.push((path.clone(), xy));
                }
            }
            for entry in porcelain {
                if seen.insert(entry.path.clone()) {
                    paths.push((entry.path.clone(), Some(entry.xy)));
                }
            }
            flatten_tree(&build_tree(&paths), collapsed, filter, width)
        }
    }
}

fn chars(xy: &[char; 2]) -> String {
    format!("{}{}", xy[0], xy[1])
}

fn matches_filter(path: &str, filter: Option<&str>) -> bool {
    match filter {
        None | Some("") => true,
        Some(q) => path.to_ascii_lowercase().contains(&q.to_ascii_lowercase()),
    }
}

fn build_tree(paths: &[(String, Option<[char; 2]>)]) -> Node {
    let mut root = Node {
        name: String::new(),
        path: String::new(),
        dir: true,
        xy: None,
        children: Vec::new(),
    };
    for (path, xy) in paths {
        insert(&mut root, path, *xy);
    }
    sort_tree(&mut root);
    root
}

fn insert(node: &mut Node, path: &str, xy: Option<[char; 2]>) {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    insert_parts(node, &parts, 0, xy);
}

fn insert_parts(node: &mut Node, parts: &[&str], at: usize, xy: Option<[char; 2]>) {
    if at >= parts.len() {
        return;
    }
    let name = parts[at];
    let path = parts[..=at].join("/");
    let last = at + 1 == parts.len();
    if let Some(child) = node.children.iter_mut().find(|c| c.name == name) {
        if last {
            child.dir = false;
            child.xy = xy;
        } else {
            child.dir = true;
            insert_parts(child, parts, at + 1, xy);
        }
        return;
    }
    let mut child = Node {
        name: name.to_string(),
        path,
        dir: !last,
        xy: if last { xy } else { None },
        children: Vec::new(),
    };
    if !last {
        insert_parts(&mut child, parts, at + 1, xy);
    }
    node.children.push(child);
}

fn sort_tree(node: &mut Node) {
    node.children.sort_by(|a, b| match (a.dir, b.dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    for child in &mut node.children {
        sort_tree(child);
    }
}

fn flatten_tree(
    root: &Node,
    collapsed: &HashSet<String>,
    filter: Option<&str>,
    width: usize,
) -> Vec<FilesRow> {
    let mut rows = Vec::new();
    for child in &root.children {
        walk(child, 0, collapsed, filter, width, &mut rows);
    }
    rows
}

fn walk(
    node: &Node,
    depth: usize,
    collapsed: &HashSet<String>,
    filter: Option<&str>,
    width: usize,
    rows: &mut Vec<FilesRow>,
) {
    if !node_visible(node, filter) {
        return;
    }
    if node.dir {
        let expanded = !collapsed.contains(&node.path);
        let display = truncate(&format!("{}{}/", indent(depth), node.name), width);
        rows.push(FilesRow {
            path: node.path.clone(),
            display,
            xy: "  ".into(),
            kind: RowKind::Directory,
            color: RowColor::Default,
            depth,
            expanded: Some(expanded),
        });
        if expanded {
            for child in &node.children {
                walk(child, depth + 1, collapsed, filter, width, rows);
            }
        }
    } else {
        let xy = node.xy.unwrap_or([' ', ' ']);
        let display = truncate(
            &format!("{}{} {}", indent(depth), chars(&xy), node.name),
            width,
        );
        rows.push(FilesRow {
            path: node.path.clone(),
            display,
            xy: chars(&xy),
            kind: RowKind::File,
            color: row_color(xy),
            depth,
            expanded: None,
        });
    }
}

fn node_visible(node: &Node, filter: Option<&str>) -> bool {
    if matches_filter(&node.path, filter) {
        return true;
    }
    node.children.iter().any(|c| node_visible(c, filter))
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn truncate(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

/// Files a focused folder should stack, in tree order.
pub(crate) fn folder_stack(
    porcelain: &[PorcelainEntry],
    ls_files: &[String],
    mode: FilesMode,
    folder: &str,
    filter: Option<&str>,
) -> Vec<(String, Option<[char; 2]>)> {
    let rows = visible_rows(porcelain, ls_files, tree_mode(mode), &HashSet::new(), filter, 256);
    rows.into_iter()
        .filter(|r| r.kind == RowKind::File && is_under(&r.path, folder))
        .map(|r| {
            let xy = if r.xy.trim().is_empty() {
                None
            } else {
                let mut chars = r.xy.chars();
                Some([chars.next().unwrap_or(' '), chars.next().unwrap_or(' ')])
            };
            (r.path, xy)
        })
        .collect()
}

fn tree_mode(mode: FilesMode) -> FilesMode {
    match mode {
        FilesMode::ChangedFlat | FilesMode::ChangedTree => FilesMode::ChangedTree,
        FilesMode::AllTree => FilesMode::AllTree,
    }
}

fn is_under(path: &str, folder: &str) -> bool {
    path == folder || path.starts_with(&format!("{folder}/"))
}
