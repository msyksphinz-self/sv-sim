//! 診断表示のゴールデンテスト (plan.md §4.2、受け入れ基準 C-2、タスク M0-T10)。
//!
//! ソースの例は IEEE 1800-2023 の構文 (§23.2 モジュール定義、§10.3 連続代入) を使う。

use serde_json::json;
use svsim_common::diag::{
    DiagCode, Diagnostic, FixIt, JSON_SCHEMA_VERSION, Label, Severity, render_human, render_json,
};
use svsim_common::source::{FileId, FileOrigin, SourceMap, Span};

/// 行の先頭オフセット: 1 行目 0、2 行目 12、3 行目 26、4 行目 42、5 行目 59。
const TOP: &str =
    "module top;\n  logic a, b;\n  assign a = c;\n  assign b = a +\n    1;\nendmodule\n";
/// 3 行目の `a` (左辺) と `c`。
const A_LHS: u32 = 26 + 9;
const C_USE: u32 = 26 + 13;

fn setup() -> (SourceMap, FileId, FileId) {
    let mut sm = SourceMap::new();
    let top = sm.add_file(
        FileOrigin::Disk("rtl/top.sv".into()),
        TOP.as_bytes().to_vec(),
    );
    let inc = sm.add_file(
        FileOrigin::Include {
            path: "rtl/defs.svh".into(),
            from: Span::new(top, 0, 0),
        },
        b"`define W 8\r\nlogic c;\r\n".to_vec(),
    );
    (sm, top, inc)
}

/// 主ラベル + 同じ行の副ラベル + 別ファイルの副ラベル + 注記 + fix-it。
fn full_diag(top: FileId, inc: FileId) -> Diagnostic {
    Diagnostic::error("`c` is used before its declaration")
        .with_code(DiagCode("E0102"))
        .with_label(Label::primary(
            Span::new(top, C_USE, C_USE + 1),
            "used here",
        ))
        .with_label(Label::secondary(
            Span::new(top, A_LHS, A_LHS + 1),
            "assigned to this",
        ))
        // defs.svh の 2 行目 (CRLF の後ろ、オフセット 13) の `c` (13 + 6 = 19)
        .with_label(Label::secondary(Span::new(inc, 19, 20), "declared here"))
        .with_note("IEEE 1800-2023 §6.18: declare before use")
        .with_fixit(FixIt {
            span: Span::new(top, 12, 12),
            replacement: "  logic c;\n".into(),
            message: "declare `c` first".into(),
        })
}

/// 4 行目の `a +` から 5 行目の `1` まで。
fn multiline_span(top: FileId) -> Span {
    let lo = TOP.find("a +").unwrap() as u32;
    let hi = TOP.find("1;").unwrap() as u32 + 1;
    Span::new(top, lo, hi)
}

#[test]
fn json_full() {
    let (sm, top, inc) = setup();
    let d = full_diag(top, inc);
    assert_eq!(JSON_SCHEMA_VERSION, 1);
    assert_eq!(
        render_json(&sm, &d),
        json!({
            "version": 1,
            "severity": "error",
            "code": "E0102",
            "message": "`c` is used before its declaration",
            "labels": [
                {"file": "rtl/top.sv", "line": 3, "col": 14, "end_line": 3, "end_col": 15,
                 "message": "used here", "primary": true},
                {"file": "rtl/top.sv", "line": 3, "col": 10, "end_line": 3, "end_col": 11,
                 "message": "assigned to this", "primary": false},
                {"file": "rtl/defs.svh", "line": 2, "col": 7, "end_line": 2, "end_col": 8,
                 "message": "declared here", "primary": false},
            ],
            "notes": ["IEEE 1800-2023 §6.18: declare before use"],
            "fixits": [
                {"file": "rtl/top.sv", "line": 2, "col": 1, "end_line": 2, "end_col": 1,
                 "replacement": "  logic c;\n", "message": "declare `c` first"},
            ],
        })
    );
}

#[test]
fn json_multiline_span() {
    let (sm, top, _) = setup();
    let d = Diagnostic::new(Severity::Warning, "width mismatch")
        .with_label(Label::primary(multiline_span(top), "33-bit expression"));
    assert_eq!(
        render_json(&sm, &d),
        json!({
            "version": 1, "severity": "warning", "code": null, "message": "width mismatch",
            "labels": [{"file": "rtl/top.sv", "line": 4, "col": 14, "end_line": 5, "end_col": 6,
                        "message": "33-bit expression", "primary": true}],
            "notes": [], "fixits": [],
        })
    );
}

#[test]
fn json_round_trips_through_text() {
    let (sm, top, inc) = setup();
    let v = render_json(&sm, &full_diag(top, inc));
    let s = serde_json::to_string(&v).unwrap();
    let back: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert_eq!(back, v);
}

#[test]
fn human_full() {
    let (sm, top, inc) = setup();
    let d = full_diag(top, inc);
    let expected = "\
error[E0102]: `c` is used before its declaration
 --> rtl/top.sv:3:14
  |
3 |   assign a = c;
  |          - assigned to this
  |              ^ used here
 ::: rtl/defs.svh:2:7
2 | logic c;
  |       - declared here
  |
  = note: IEEE 1800-2023 §6.18: declare before use
";
    assert_eq!(render_human(&sm, &d), expected);
}

#[test]
fn human_multiline_span_shows_start_line_only() {
    let (sm, top, _) = setup();
    let d = Diagnostic::new(Severity::Warning, "width mismatch")
        .with_label(Label::primary(multiline_span(top), "33-bit expression"));
    let expected = "\
warning: width mismatch
 --> rtl/top.sv:4:14
  |
4 |   assign b = a +
  |              ^^^ 33-bit expression
";
    assert_eq!(render_human(&sm, &d), expected);
}

#[test]
fn human_tab_and_wide_gutter() {
    let mut sm = SourceMap::new();
    let mut text = "\n".repeat(99);
    text.push_str("\twire [7:0] bus;\n");
    let f = sm.add_file(FileOrigin::Disk("w.sv".into()), text.into_bytes());
    let lo = 99 + "\twire [7:0] ".len() as u32;
    let d = Diagnostic::new(Severity::Note, "net declared")
        .with_label(Label::primary(Span::new(f, lo, lo + 3), ""))
        .with_note("first")
        .with_note("second");
    let expected = "\
note: net declared
   --> w.sv:100:13
    |
100 |     wire [7:0] bus;
    |                ^^^
    |
    = note: first
    = note: second
";
    assert_eq!(render_human(&sm, &d), expected);
}

#[test]
fn human_empty_span_at_eof() {
    let mut sm = SourceMap::new();
    let f = sm.add_file(FileOrigin::Disk("e.sv".into()), b"module m".to_vec());
    let d =
        Diagnostic::error("expected `;`").with_label(Label::primary(Span::new(f, 8, 8), "here"));
    let expected = "\
error: expected `;`
 --> e.sv:1:9
  |
1 | module m
  |         ^ here
";
    assert_eq!(render_human(&sm, &d), expected);
}

#[test]
fn human_utf8_and_invalid_bytes() {
    let mut sm = SourceMap::new();
    // コメント中の日本語と不正なバイト (\xff) の後ろの識別子
    let bytes = b"// \xe6\xb3\xa8\xe6\x84\x8f\xff y;\n".to_vec();
    let lo = bytes.len() as u32 - 3; // `y`
    let f = sm.add_file(FileOrigin::Disk("u.sv".into()), bytes);
    let d = Diagnostic::error("e").with_label(Label::primary(Span::new(f, lo, lo + 1), ""));
    let expected = "\
error: e
 --> u.sv:1:8
  |
1 | // 注意\u{fffd} y;
  |        ^
";
    assert_eq!(render_human(&sm, &d), expected);
}
