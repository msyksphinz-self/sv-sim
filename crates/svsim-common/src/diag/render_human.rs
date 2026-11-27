//! 診断の人間向け単一行表示 (plan.md §4.2、タスク M0-T10)。
//!
//! 形式 (rustc に似せる):
//! ```text
//! error[E0001]: message
//!  --> file.sv:2:8
//!   |
//! 2 | assign a = b;
//!   |        ^ primary label
//!   |            - secondary label
//!   |
//!   = note: note text
//! ```
//! - ラベルは位置 (ファイル、開始位置) の順に表示する。同じ行のラベルは、ソース行を 1 回だけ
//!   表示して下線の行を並べる。主ラベルの下線は `^`、副ラベルは `-`。
//! - 主ラベルと別のファイルのラベルの前には ` ::: file:line:col` を出す。
//! - 複数行にまたがる span は、開始位置と開始行だけを表示する。下線は開始列から行末まで。
//! - 長さ 0 の span は下線 1 文字。
//! - ソース行は損失のある UTF-8 変換で表示する。不正なバイトは 1 バイトを U+FFFD 1 文字にする
//!   (列の数え方 [`crate::source::LineCol`] と揃えるため)。タブは空白 4 つにする。
//!   全角文字の表示幅は考慮しない (1 文字 = 1 桁)。
//! - 色付け、複数行の描画、fix-it の表示は M1 で入れる。

use super::Diagnostic;
use crate::source::{SourceMap, Span};
use std::fmt::Write as _;

/// タブを置き換える空白の数。
const TAB_WIDTH: usize = 4;

/// 診断を人間向けの文字列にする。各行は `\n` で終わる。
pub fn render_human(sm: &SourceMap, d: &Diagnostic) -> String {
    let mut out = String::new();

    // ヘッダ
    out.push_str(d.severity.as_str());
    if let Some(code) = d.code {
        let _ = write!(out, "[{}]", code.0);
    }
    let _ = writeln!(out, ": {}", d.message);

    // ラベルを位置の順に並べる (同じ位置なら主ラベルを先に、そのあと元の順)。
    let mut labels: Vec<_> = d.labels.iter().collect();
    labels.sort_by_key(|l| (l.span.file, l.span.lo, !l.primary));

    // 行番号の桁数
    let max_line = labels
        .iter()
        .map(|l| sm.line_col(l.span.file, l.span.lo).line)
        .max();
    let w = max_line.map_or(0, |n| n.to_string().len());
    let pad = " ".repeat(w);

    if let Some(primary) = d.primary_span() {
        let _ = writeln!(out, "{pad}--> {}", location(sm, primary));
        let _ = writeln!(out, "{pad} |");
        let mut cur_file = primary.file;
        let mut prev_line: Option<(crate::source::FileId, u32)> = None;
        for l in &labels {
            let file = sm.file(l.span.file);
            let start = file.line_col(l.span.lo);
            if l.span.file != cur_file {
                let _ = writeln!(out, "{pad}::: {}", location(sm, l.span));
                cur_file = l.span.file;
                prev_line = None;
            }
            let line_bytes = file.line_text(start.line);
            let (display, char_cols) = display_line(line_bytes);
            if prev_line != Some((l.span.file, start.line)) {
                let _ = writeln!(out, "{:>w$} | {}", start.line, display);
                prev_line = Some((l.span.file, start.line));
            }
            // 下線の範囲 (文字単位、行内)。
            let line_start = file.line_start(start.line);
            let line_chars = char_cols.len() - 1;
            let c0 = (start.col as usize - 1).min(line_chars);
            let c1 = if l.span.hi as usize <= line_start as usize + line_bytes.len() {
                let end = file.line_col(l.span.hi);
                if end.line == start.line {
                    (end.col as usize - 1).min(line_chars)
                } else {
                    line_chars
                }
            } else {
                // 複数行の span: 行末まで
                line_chars
            };
            let n = c1.saturating_sub(c0).max(1);
            let indent = char_cols[c0];
            let width = (char_cols[(c0 + n).min(line_chars)] - indent).max(n);
            let mark = if l.primary { "^" } else { "-" };
            let _ = write!(out, "{pad} | {}{}", " ".repeat(indent), mark.repeat(width));
            if !l.message.is_empty() {
                let _ = write!(out, " {}", l.message);
            }
            out.push('\n');
        }
        if !d.notes.is_empty() {
            let _ = writeln!(out, "{pad} |");
        }
    }

    for note in &d.notes {
        let _ = writeln!(out, "{pad} = note: {note}");
    }
    out
}

/// `file:line:col` (span の開始位置)。
fn location(sm: &SourceMap, span: Span) -> String {
    let lc = sm.line_col(span.file, span.lo);
    format!(
        "{}:{}:{}",
        sm.file(span.file).display_name(),
        lc.line,
        lc.col
    )
}

/// 行のバイト列を表示用の文字列にする。
///
/// 戻り値の 2 つ目は、各文字の表示上の開始桁 (末尾に行全体の幅を足したもの。長さは文字数 + 1)。
fn display_line(bytes: &[u8]) -> (String, Vec<usize>) {
    let mut s = String::new();
    let mut cols = vec![0usize];
    let mut col = 0usize;
    for chunk in bytes.utf8_chunks() {
        for ch in chunk.valid().chars() {
            if ch == '\t' {
                s.push_str(&" ".repeat(TAB_WIDTH));
                col += TAB_WIDTH;
            } else {
                s.push(ch);
                col += 1;
            }
            cols.push(col);
        }
        for _ in chunk.invalid() {
            s.push(char::REPLACEMENT_CHARACTER);
            col += 1;
            cols.push(col);
        }
    }
    (s, cols)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::{DiagCode, FixIt, Label, Severity};
    use crate::source::FileOrigin;

    fn sm_one(text: &str) -> (SourceMap, crate::source::FileId) {
        let mut sm = SourceMap::new();
        let f = sm.add_file(FileOrigin::Disk("t.sv".into()), text.as_bytes().to_vec());
        (sm, f)
    }

    #[test]
    fn header_only() {
        let (sm, _) = sm_one("");
        let d = Diagnostic::error("no top module").with_note("specify --top");
        assert_eq!(
            render_human(&sm, &d),
            "error: no top module\n = note: specify --top\n"
        );
    }

    #[test]
    fn single_primary_label() {
        let (sm, f) = sm_one("module m;\n  assign a = b;\nendmodule\n");
        // "b" は 2 行目の 14 バイト目 (オフセット 10 + 13 = 23)
        let d = Diagnostic::error("undeclared identifier `b`")
            .with_code(DiagCode("E0001"))
            .with_label(Label::primary(Span::new(f, 23, 24), "not found"));
        assert_eq!(
            render_human(&sm, &d),
            "error[E0001]: undeclared identifier `b`\n\
             \x20--> t.sv:2:14\n\
             \x20 |\n\
             2 |   assign a = b;\n\
             \x20 |              ^ not found\n"
        );
    }

    #[test]
    fn fixits_are_not_rendered() {
        let (sm, f) = sm_one("x\n");
        let d = Diagnostic::new(Severity::Warning, "w")
            .with_label(Label::primary(Span::new(f, 0, 1), ""))
            .with_fixit(FixIt {
                span: Span::new(f, 0, 1),
                replacement: "y".into(),
                message: "rename".into(),
            });
        assert_eq!(
            render_human(&sm, &d),
            "warning: w\n --> t.sv:1:1\n  |\n1 | x\n  | ^\n"
        );
    }

    #[test]
    fn display_line_tabs_and_invalid() {
        let (s, cols) = display_line(b"\ta\xffb");
        assert_eq!(s, "    a\u{fffd}b");
        assert_eq!(cols, vec![0, 4, 5, 6, 7]);
    }
}
