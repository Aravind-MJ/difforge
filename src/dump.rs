//! Diff pane body: stacked dumps, failure text, side-by-side padding.

use ansi_to_tui::IntoText as _;
use ratatui::text::{Line, Span, Text};
use unicode_width::UnicodeWidthChar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FailKind {
    Git,
    Difft,
}

pub(crate) fn bytes_to_text(bytes: &[u8], pane: u16) -> Text<'static> {
    if bytes.contains(&0) {
        return text_from_string("Binary contents changed.");
    }
    match bytes.to_vec().into_text() {
        Ok(text) if !text.lines.is_empty() => widen_columns(clip_lines(text, pane), pane),
        Ok(_) => Text::default(),
        Err(_) => Text::from(String::from_utf8_lossy(bytes).into_owned()),
    }
}

pub(crate) fn failure_text(stderr: &[u8], stdout: &[u8], exit: i32, kind: FailKind) -> String {
    let mut text = String::from_utf8_lossy(stderr).into_owned();
    text.push_str(&String::from_utf8_lossy(stdout));
    if text.is_empty() {
        match kind {
            FailKind::Git => format!("git failed (exit {exit})"),
            FailKind::Difft => format!("difft failed (exit {exit})"),
        }
    } else {
        text
    }
}

pub(crate) fn text_from_string(s: impl Into<String>) -> Text<'static> {
    Text::from(s.into())
}

pub(crate) fn pane_string(text: &Text<'_>) -> String {
    text.lines
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn stack_texts(parts: Vec<Text<'static>>) -> Text<'static> {
    let mut lines = Vec::new();
    for (i, part) in parts.into_iter().enumerate() {
        if i > 0 && !lines.is_empty() && !part.lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.extend(part.lines);
    }
    Text::from(lines)
}

fn clip_lines(text: Text<'static>, pane: u16) -> Text<'static> {
    let max_w = pane.max(1) as usize;
    Text::from(
        text.lines
            .into_iter()
            .map(|line| clip_line(line, max_w))
            .collect::<Vec<_>>(),
    )
}

fn clip_line(line: Line<'static>, max_w: usize) -> Line<'static> {
    let mut used = 0;
    let mut spans = Vec::new();
    for span in line.spans {
        let w = span.width();
        if used >= max_w {
            break;
        }
        if used + w <= max_w {
            used += w;
            spans.push(span);
            continue;
        }
        let mut cut = String::new();
        for ch in span.content.chars() {
            let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used + cw > max_w {
                break;
            }
            cut.push(ch);
            used += cw;
        }
        if !cut.is_empty() {
            spans.push(Span::styled(cut, span.style));
        }
        break;
    }
    Line::from(spans)
}

fn widen_columns(text: Text<'static>, pane: u16) -> Text<'static> {
    Text::from(
        text.lines
            .into_iter()
            .map(|line| widen_line(line, pane))
            .collect::<Vec<_>>(),
    )
}

fn is_line_num_span(content: &str) -> bool {
    let bytes = content.as_bytes();
    if !(2..=8).contains(&bytes.len()) || *bytes.last().unwrap() != b' ' {
        return false;
    }
    let inner = &content[..content.len() - 1];
    let mut digit = false;
    for c in inner.chars() {
        if c.is_ascii_digit() {
            digit = true;
        } else if c != '.' && c != ' ' {
            return false;
        }
    }
    digit
}

fn widen_line(line: Line<'static>, pane: u16) -> Line<'static> {
    let mut first = None;
    let mut rhs_at = None;
    for (i, span) in line.spans.iter().enumerate() {
        if is_line_num_span(span.content.as_ref()) {
            if first.is_none() {
                first = Some(i);
            } else {
                rhs_at = Some(i);
                break;
            }
        }
    }
    let (Some(first), Some(rhs_at)) = (first, rhs_at) else {
        return line;
    };
    let between: usize = line.spans[first + 1..rhs_at].iter().map(Span::width).sum();
    if between == 0 {
        return line;
    }
    let lhs_w: usize = line.spans[..rhs_at].iter().map(Span::width).sum();
    let target_rhs = (pane.saturating_sub(1) / 2).saturating_add(1) as usize;
    if lhs_w >= target_rhs {
        return line;
    }
    let mut spans = line.spans;
    spans.insert(rhs_at, Span::raw(" ".repeat(target_rhs - lhs_w)));
    Line::from(spans)
}
