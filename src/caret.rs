//! Caret motion for one-line and multiline fields.

pub(crate) fn char_len(s: &str) -> usize {
    s.chars().count()
}

fn byte_at(s: &str, caret: usize) -> usize {
    s.char_indices().nth(caret).map(|(i, _)| i).unwrap_or(s.len())
}

pub(crate) fn insert_char(s: &mut String, caret: &mut usize, c: char) {
    let i = byte_at(s, *caret);
    s.insert(i, c);
    *caret += 1;
}

pub(crate) fn delete_before(s: &mut String, caret: &mut usize) {
    if *caret == 0 {
        return;
    }
    *caret -= 1;
    let i = byte_at(s, *caret);
    let n = s[i..].chars().next().map(char::len_utf8).unwrap_or(0);
    s.replace_range(i..i + n, "");
}

pub(crate) fn delete_after(s: &mut String, caret: usize) {
    let i = byte_at(s, caret);
    if i >= s.len() {
        return;
    }
    let n = s[i..].chars().next().map(char::len_utf8).unwrap_or(0);
    s.replace_range(i..i + n, "");
}

pub(crate) fn move_caret(caret: &mut usize, len: usize, delta: isize) {
    let next = (*caret as isize + delta).clamp(0, len as isize);
    *caret = next as usize;
}

pub(crate) fn visual_line_starts(text: &str, width: u16) -> Vec<usize> {
    let w = width.max(1) as usize;
    let mut starts = vec![0];
    let mut col = 0;
    for (i, ch) in text.chars().enumerate() {
        if ch == '\n' {
            starts.push(i + 1);
            col = 0;
        } else {
            col += 1;
            if col >= w {
                starts.push(i + 1);
                col = 0;
            }
        }
    }
    starts
}

fn line_end(starts: &[usize], line: usize, total: usize) -> usize {
    starts.get(line + 1).copied().unwrap_or(total)
}

pub(crate) fn move_caret_vert(text: &str, caret: usize, width: u16, dy: isize) -> usize {
    let total = char_len(text);
    let starts = visual_line_starts(text, width);
    let line = starts.iter().rposition(|&s| s <= caret).unwrap_or(0);
    let col = caret.saturating_sub(starts[line]);
    let target = (line as isize + dy).clamp(0, starts.len() as isize - 1) as usize;
    let start = starts[target];
    let end = line_end(&starts, target, total);
    let len = end.saturating_sub(start);
    let at_nl = end > start && text.chars().nth(end - 1) == Some('\n');
    let usable = if at_nl { len.saturating_sub(1) } else { len };
    start + col.min(usable)
}

pub(crate) fn caret_on_line(text: &str, click_x: u16) -> usize {
    (click_x as usize).min(char_len(text))
}

pub(crate) fn caret_in_wrapped(text: &str, click_x: u16, click_y: u16, width: u16) -> usize {
    let w = width.max(1) as usize;
    let target_x = click_x as usize;
    let target_y = click_y as usize;
    let mut y = 0usize;
    let mut x = 0usize;
    for (i, ch) in text.chars().enumerate() {
        if y > target_y {
            return i.saturating_sub(1);
        }
        if y == target_y && x >= target_x {
            return i;
        }
        if ch == '\n' {
            if y == target_y {
                return i;
            }
            y += 1;
            x = 0;
        } else {
            x += 1;
            if x >= w {
                y += 1;
                x = 0;
            }
        }
    }
    char_len(text)
}

pub(crate) fn strip_height(description_lines: usize) -> u16 {
    let lines = (description_lines.max(2) as u16).min(8);
    3 + lines
}

pub(crate) fn description_line_count(description: &str) -> usize {
    description.split('\n').count()
}
