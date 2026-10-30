use std::fmt::Write as _;

use kagura_support::Span;

use crate::Diagnostic;

pub fn render(source: &str, path: &str, diagnostic: &Diagnostic) -> String {
    let span = clamp(source, diagnostic.span);
    let (line, column) = line_column(source, span.start);
    let line_text = source.lines().nth(line.saturating_sub(1)).unwrap_or("");
    let caret_pad = column.saturating_sub(1);
    let caret_len = source
        .get(span.start as usize..span.end as usize)
        .map(|text| text.chars().count().max(1))
        .unwrap_or(1);

    let mut out = String::new();
    let _ = writeln!(out, "{path}:{line}:{column}: {}", diagnostic.kind.label());
    let _ = writeln!(out, "  {}", diagnostic.message);
    let _ = writeln!(out, "  {line_text}");
    let _ = write!(out, "  {}{}", " ".repeat(caret_pad), "^".repeat(caret_len));
    out
}

fn clamp(source: &str, span: Span) -> Span {
    let len = source.len() as u32;
    let start = span.start.min(len);
    let end = span.end.min(len).max(start);
    Span::new(start, end)
}

fn line_column(source: &str, offset: u32) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for (index, ch) in source.char_indices() {
        if index >= offset as usize {
            break;
        }
        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}
