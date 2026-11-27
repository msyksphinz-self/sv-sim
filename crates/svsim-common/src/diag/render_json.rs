//! 診断の JSON 表示 (AI 向けの構造化診断、plan.md §4.2、タスク M0-T10)。
//!
//! スキーマ (版 [`JSON_SCHEMA_VERSION`] = 1):
//! ```text
//! {"version": 1,
//!  "severity": "error" | "warning" | "note" | "help",
//!  "code": "E0001" | null,
//!  "message": string,
//!  "labels": [{"file", "line", "col", "end_line", "end_col", "message", "primary"}],
//!  "notes": [string],
//!  "fixits": [{"file", "line", "col", "end_line", "end_col", "replacement", "message"}]}
//! ```
//! - `file` はファイルの表示名 ([`crate::source::SourceFile::display_name`])。
//! - `line`/`col` は span の開始位置、`end_line`/`end_col` は span の終了位置 (`hi`、
//!   含まない側) の行・列。どちらも 1 始まりで、列は UTF-8 の文字単位
//!   ([`crate::source::LineCol`])。
//! - 配列の順序は [`Diagnostic`] の中の順序のまま。
//! - スキーマを互換性の無い形で変えるときは、`version` を上げる。

use super::Diagnostic;
use crate::source::{SourceMap, Span};
use serde_json::{Map, Value, json};

/// JSON 表示のスキーマの版。
pub const JSON_SCHEMA_VERSION: u32 = 1;

/// 診断を JSON の値にする。
pub fn render_json(sm: &SourceMap, d: &Diagnostic) -> Value {
    let labels: Vec<Value> = d
        .labels
        .iter()
        .map(|l| {
            let mut m = span_fields(sm, l.span);
            m.insert("message".into(), Value::from(l.message.as_str()));
            m.insert("primary".into(), Value::from(l.primary));
            Value::Object(m)
        })
        .collect();
    let fixits: Vec<Value> = d
        .fixits
        .iter()
        .map(|f| {
            let mut m = span_fields(sm, f.span);
            m.insert("replacement".into(), Value::from(f.replacement.as_str()));
            m.insert("message".into(), Value::from(f.message.as_str()));
            Value::Object(m)
        })
        .collect();
    json!({
        "version": JSON_SCHEMA_VERSION,
        "severity": d.severity.as_str(),
        "code": d.code.map(|c| c.0),
        "message": d.message,
        "labels": labels,
        "notes": d.notes,
        "fixits": fixits,
    })
}

/// `file`、`line`、`col`、`end_line`、`end_col` のフィールド。
fn span_fields(sm: &SourceMap, span: Span) -> Map<String, Value> {
    let file = sm.file(span.file);
    let start = file.line_col(span.lo);
    let end = file.line_col(span.hi);
    let mut m = Map::new();
    m.insert("file".into(), Value::from(file.display_name()));
    m.insert("line".into(), Value::from(start.line));
    m.insert("col".into(), Value::from(start.col));
    m.insert("end_line".into(), Value::from(end.line));
    m.insert("end_col".into(), Value::from(end.col));
    m
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::Severity;
    use crate::source::FileOrigin;

    #[test]
    fn minimal() {
        let sm = SourceMap::new();
        let d = Diagnostic::new(Severity::Help, "hi");
        assert_eq!(
            render_json(&sm, &d),
            json!({"version": 1, "severity": "help", "code": null, "message": "hi",
                   "labels": [], "notes": [], "fixits": []})
        );
    }

    #[test]
    fn span_fields_use_exclusive_end() {
        let mut sm = SourceMap::new();
        let f = sm.add_file(
            FileOrigin::Virtual {
                name: "<cmdline>".into(),
            },
            b"ab\ncd".to_vec(),
        );
        let m = span_fields(&sm, Span::new(f, 1, 4));
        assert_eq!(
            Value::Object(m),
            json!({"file": "<cmdline>", "line": 1, "col": 2, "end_line": 2, "end_col": 2})
        );
    }
}
