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

// ===========================================================================
// test-plan M0-TC63 / M0-TC64 の `a.sv` の例 (期待値は test-plan の表をそのまま写したもの)
// ===========================================================================

/// `a.sv` の行の先頭オフセット: 1 行目 0、2 行目 10、3 行目 19。
const A_SV: &str = "module m;\n  wire w\nendmodule\n";

fn a_sv() -> (SourceMap, FileId) {
    let mut sm = SourceMap::new();
    let f = sm.add_file(FileOrigin::Disk("a.sv".into()), A_SV.as_bytes().to_vec());
    (sm, f)
}

/// TC63 の入力: 主ラベル (2 行目の `w`)、副ラベル (`wire`)、複数行の副ラベル、注記、fix-it。
fn a_sv_diag(f: FileId) -> Diagnostic {
    Diagnostic::error("expected ';'")
        .with_code(DiagCode("E0001"))
        .with_label(Label::primary(Span::new(f, 17, 18), "here"))
        .with_label(Label::secondary(Span::new(f, 12, 16), "declaration"))
        .with_label(Label::secondary(Span::new(f, 10, 28), "scope"))
        .with_note("note one")
        .with_fixit(FixIt {
            span: Span::new(f, 18, 18),
            replacement: ";".into(),
            message: "insert ';'".into(),
        })
}

/// TC63 (C-2, 決定 Q-13): `end_line`/`end_col` は `hi` (排他) の位置、`severity` は小文字、
/// ラベルと fix-it は発行順。
#[test]
fn tc63_json_a_sv() {
    let (sm, f) = a_sv();
    let d = a_sv_diag(f);
    let expected = json!({
        "version": 1, "severity": "error", "code": "E0001", "message": "expected ';'",
        "labels": [
            {"file": "a.sv", "line": 2, "col": 8, "end_line": 2, "end_col": 9,
             "message": "here", "primary": true},
            {"file": "a.sv", "line": 2, "col": 3, "end_line": 2, "end_col": 7,
             "message": "declaration", "primary": false},
            {"file": "a.sv", "line": 2, "col": 1, "end_line": 3, "end_col": 10,
             "message": "scope", "primary": false},
        ],
        "notes": ["note one"],
        "fixits": [
            {"file": "a.sv", "line": 2, "col": 9, "end_line": 2, "end_col": 9,
             "replacement": ";", "message": "insert ';'"},
        ],
    });
    let v = render_json(&sm, &d);
    assert_eq!(v, expected);
    // 決定性: 2 回呼んで同じ JSON 文字列。文字列から読み戻すと同じ値。
    let s1 = serde_json::to_string(&v).unwrap();
    let s2 = serde_json::to_string(&render_json(&sm, &d)).unwrap();
    assert_eq!(s1, s2);
    let back: serde_json::Value = serde_json::from_str(&s1).unwrap();
    assert_eq!(back, expected);
}

/// TC63: code が無い診断は `"code": null`、注記と fix-it が無ければ空の配列。
#[test]
fn tc63_json_a_sv_without_code_notes_fixits() {
    let (sm, f) = a_sv();
    let d = Diagnostic::error("expected ';'").with_label(Label::primary(Span::new(f, 17, 18), ""));
    assert_eq!(
        render_json(&sm, &d),
        json!({
            "version": 1, "severity": "error", "code": null, "message": "expected ';'",
            "labels": [{"file": "a.sv", "line": 2, "col": 8, "end_line": 2, "end_col": 9,
                        "message": "", "primary": true}],
            "notes": [], "fixits": [],
        })
    );
}

/// TC64 の追加ゴールデン 1: 主ラベル `here` + 同じ行の副ラベル `declaration` + 注記 (規則 1〜5, 8)。
#[test]
fn tc64_human_a_sv_primary_secondary_note() {
    let (sm, f) = a_sv();
    let d = Diagnostic::error("expected ';'")
        .with_code(DiagCode("E0001"))
        .with_label(Label::primary(Span::new(f, 17, 18), "here"))
        .with_label(Label::secondary(Span::new(f, 12, 16), "declaration"))
        .with_note("note one");
    let expected = "\
error[E0001]: expected ';'
 --> a.sv:2:8
  |
2 |   wire w
  |   ---- declaration
  |        ^ here
  |
  = note: note one
";
    assert_eq!(render_human(&sm, &d), expected);
}

/// TC64 の追加ゴールデン 2: 複数行の主ラベルは開始行だけを、開始位置から行末まで印を付ける (規則 6)。
#[test]
fn tc64_human_a_sv_multiline_primary() {
    let (sm, f) = a_sv();
    let d = Diagnostic::error("in scope").with_label(Label::primary(Span::new(f, 10, 28), "scope"));
    let expected = "\
error: in scope
 --> a.sv:2:1
  |
2 |   wire w
  | ^^^^^^^^ scope
";
    assert_eq!(render_human(&sm, &d), expected);
}

/// TC64 の追加ゴールデン 3: マルチバイト文字の後ろの列は文字単位 (`あ` は 3 バイトで 1 文字、規則 5)。
/// message が空なら印の後ろに空白を付けない。
#[test]
fn tc64_human_multibyte_column() {
    let mut sm = SourceMap::new();
    let f = sm.add_file(
        FileOrigin::Disk("m.sv".into()),
        "  wire あw\n".as_bytes().to_vec(),
    );
    let d = Diagnostic::error("e").with_label(Label::primary(Span::new(f, 10, 11), ""));
    let expected = "\
error: e
 --> m.sv:1:9
  |
1 |   wire あw
  |         ^
";
    assert_eq!(render_human(&sm, &d), expected);
}
