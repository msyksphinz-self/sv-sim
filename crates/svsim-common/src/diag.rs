//! 診断: 重大度、ラベル、fix-it、診断の収集 (plan.md §4.2、タスク M0-T10)。
//!
//! 表示は 2 種類ある。
//! - [`render_human`]: 人間向けの表示。M0 は単一行の span だけに対応する
//!   (複数行の span は開始位置と開始行だけを表示する)。色付け、複数行の描画、
//!   fix-it の表示は M1 で入れる。
//! - [`render_json`]: AI 向けの構造化診断 (PLAN §8.4)。スキーマの版は
//!   [`JSON_SCHEMA_VERSION`]。

mod render_human;
mod render_json;

pub use render_human::render_human;
pub use render_json::{JSON_SCHEMA_VERSION, render_json};

use crate::source::Span;
use std::cmp::Ordering;

/// 診断の重大度。順序 (`Ord`) は Error < Warning < Note < Help。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub enum Severity {
    /// エラー。
    Error,
    /// 警告。
    Warning,
    /// 注記。
    Note,
    /// 助言。
    Help,
}

impl Severity {
    /// 表示と JSON に使う名前 (`"error"`、`"warning"`、`"note"`、`"help"`)。
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Note => "note",
            Severity::Help => "help",
        }
    }
}

/// 診断コード (例: `"E0001"`)。番号体系の割り当ては M1 で決める。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct DiagCode(pub &'static str);

/// ソース上の位置に付けるラベル。
#[derive(Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Label {
    /// 位置。
    pub span: Span,
    /// メッセージ (空でもよい)。
    pub message: String,
    /// 主ラベルなら `true`。主ラベルは診断の位置を決める。
    pub primary: bool,
}

impl Label {
    /// 主ラベル。
    pub fn primary(span: Span, message: impl Into<String>) -> Self {
        Label {
            span,
            message: message.into(),
            primary: true,
        }
    }

    /// 副ラベル。
    pub fn secondary(span: Span, message: impl Into<String>) -> Self {
        Label {
            span,
            message: message.into(),
            primary: false,
        }
    }
}

/// 修正案: `span` を `replacement` で置き換える。
#[derive(Clone, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FixIt {
    /// 置き換える範囲 (長さ 0 なら挿入)。
    pub span: Span,
    /// 置き換え後のテキスト。
    pub replacement: String,
    /// 説明。
    pub message: String,
}

/// 1 件の診断。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Diagnostic {
    /// 重大度。
    pub severity: Severity,
    /// 診断コード。
    pub code: Option<DiagCode>,
    /// 主メッセージ。
    pub message: String,
    /// ラベル。主ラベルは普通 1 つ。
    pub labels: Vec<Label>,
    /// 注記。
    pub notes: Vec<String>,
    /// 修正案。
    pub fixits: Vec<FixIt>,
}

impl Diagnostic {
    /// 重大度とメッセージだけの診断を作る。
    pub fn new(severity: Severity, message: impl Into<String>) -> Self {
        Diagnostic {
            severity,
            code: None,
            message: message.into(),
            labels: Vec::new(),
            notes: Vec::new(),
            fixits: Vec::new(),
        }
    }

    /// エラーの診断を作る。
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(Severity::Error, message)
    }

    /// 警告の診断を作る。
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(Severity::Warning, message)
    }

    /// 診断コードを付ける。
    pub fn with_code(mut self, code: DiagCode) -> Self {
        self.code = Some(code);
        self
    }

    /// ラベルを足す。
    pub fn with_label(mut self, label: Label) -> Self {
        self.labels.push(label);
        self
    }

    /// 注記を足す。
    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.notes.push(note.into());
        self
    }

    /// 修正案を足す。
    pub fn with_fixit(mut self, fixit: FixIt) -> Self {
        self.fixits.push(fixit);
        self
    }

    /// 診断の位置: 最初の主ラベルの span。主ラベルが無ければ最初のラベルの span。
    /// ラベルが無ければ `None`。
    pub fn primary_span(&self) -> Option<Span> {
        self.labels
            .iter()
            .find(|l| l.primary)
            .or_else(|| self.labels.first())
            .map(|l| l.span)
    }

    /// 決定的な並べ替えに使う全順序。
    ///
    /// (位置、重大度、コード、メッセージ、ラベル、注記、修正案) の辞書式。
    /// 位置の無い診断は、位置のある診断より前に来る。
    /// 内容が全部同じ診断どうしは区別できないので、発行順に依らず同じ結果になる。
    fn sort_cmp(&self, other: &Self) -> Ordering {
        self.primary_span()
            .cmp(&other.primary_span())
            .then_with(|| self.severity.cmp(&other.severity))
            .then_with(|| self.code.cmp(&other.code))
            .then_with(|| self.message.cmp(&other.message))
            .then_with(|| self.labels.cmp(&other.labels))
            .then_with(|| self.notes.cmp(&other.notes))
            .then_with(|| self.fixits.cmp(&other.fixits))
    }
}

/// 診断を集める入れ物。
#[derive(Clone, Debug, Default)]
pub struct DiagSink {
    diags: Vec<Diagnostic>,
    errors: usize,
    warnings: usize,
}

impl DiagSink {
    /// 空の入れ物。
    pub fn new() -> Self {
        Self::default()
    }

    /// 診断を 1 件追加する。
    pub fn emit(&mut self, d: Diagnostic) {
        match d.severity {
            Severity::Error => self.errors += 1,
            Severity::Warning => self.warnings += 1,
            Severity::Note | Severity::Help => {}
        }
        self.diags.push(d);
    }

    /// エラーが 1 件以上あれば `true`。
    pub fn has_errors(&self) -> bool {
        self.errors > 0
    }

    /// エラーの件数。
    pub fn error_count(&self) -> usize {
        self.errors
    }

    /// 警告の件数。
    pub fn warning_count(&self) -> usize {
        self.warnings
    }

    /// 診断の件数。
    pub fn len(&self) -> usize {
        self.diags.len()
    }

    /// 診断が無ければ `true`。
    pub fn is_empty(&self) -> bool {
        self.diags.is_empty()
    }

    /// 発行順の診断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diags
    }

    /// 決定的な順序に並べて返す。並列パースでも出力順を一定にするため。
    ///
    /// 第 1 キーは位置 (ファイル ID、開始位置、終了位置)。位置の無い診断が先頭に来る。
    /// 位置が同じなら重大度、コード、メッセージ、ラベル、注記、修正案の順に比べる。
    /// それも全部同じなら発行順 (安定ソート)。内容が全部同じ診断は区別できないので、
    /// 結果は発行順の入れ替えに依存しない。
    pub fn into_sorted(self) -> Vec<Diagnostic> {
        let mut v = self.diags;
        v.sort_by(Diagnostic::sort_cmp);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{FileOrigin, SourceMap};

    fn two_files() -> (crate::source::FileId, crate::source::FileId) {
        let mut sm = SourceMap::new();
        let a = sm.add_file(FileOrigin::Disk("a".into()), vec![b' '; 100]);
        let b = sm.add_file(FileOrigin::Disk("b".into()), vec![b' '; 100]);
        (a, b)
    }

    #[test]
    fn severity_names() {
        assert_eq!(Severity::Error.as_str(), "error");
        assert_eq!(Severity::Warning.as_str(), "warning");
        assert_eq!(Severity::Note.as_str(), "note");
        assert_eq!(Severity::Help.as_str(), "help");
    }

    #[test]
    fn counts() {
        let mut s = DiagSink::new();
        assert!(!s.has_errors());
        assert!(s.is_empty());
        s.emit(Diagnostic::warning("w"));
        assert!(!s.has_errors());
        s.emit(Diagnostic::new(Severity::Note, "n"));
        s.emit(Diagnostic::error("e"));
        assert!(s.has_errors());
        assert_eq!(s.error_count(), 1);
        assert_eq!(s.warning_count(), 1);
        assert_eq!(s.len(), 3);
        assert_eq!(s.diagnostics()[0].message, "w");
    }

    #[test]
    fn primary_span_choice() {
        let (a, _) = two_files();
        let s1 = Span::new(a, 1, 2);
        let s2 = Span::new(a, 5, 6);
        let d = Diagnostic::error("x")
            .with_label(Label::secondary(s1, ""))
            .with_label(Label::primary(s2, ""));
        assert_eq!(d.primary_span(), Some(s2));
        let d = Diagnostic::error("x").with_label(Label::secondary(s1, ""));
        assert_eq!(d.primary_span(), Some(s1));
        assert_eq!(Diagnostic::error("x").primary_span(), None);
    }

    #[test]
    fn into_sorted_orders_by_file_then_position() {
        let (a, b) = two_files();
        let mut s = DiagSink::new();
        s.emit(Diagnostic::error("b0").with_label(Label::primary(Span::new(b, 0, 1), "")));
        s.emit(Diagnostic::error("a9").with_label(Label::primary(Span::new(a, 9, 10), "")));
        s.emit(Diagnostic::error("a2").with_label(Label::primary(Span::new(a, 2, 3), "")));
        s.emit(Diagnostic::error("none"));
        let msgs: Vec<_> = s.into_sorted().into_iter().map(|d| d.message).collect();
        assert_eq!(msgs, ["none", "a2", "a9", "b0"]);
    }

    /// C-3: 発行順をすべて入れ替えても、並べた結果が同じになる。
    #[test]
    fn into_sorted_is_independent_of_emission_order() {
        let (a, b) = two_files();
        let sp = |f, lo| Span::new(f, lo, lo + 1);
        let base = vec![
            Diagnostic::error("x").with_label(Label::primary(sp(a, 3), "")),
            // 同じ位置で内容の違うもの
            Diagnostic::warning("x").with_label(Label::primary(sp(a, 3), "")),
            Diagnostic::error("y").with_label(Label::primary(sp(a, 3), "")),
            Diagnostic::error("y")
                .with_code(DiagCode("E1"))
                .with_label(Label::primary(sp(a, 3), "")),
            Diagnostic::error("y")
                .with_label(Label::primary(sp(a, 3), ""))
                .with_note("n"),
            Diagnostic::error("z").with_label(Label::primary(sp(b, 0), "")),
            Diagnostic::error("g"),
            // 完全に同じもの
            Diagnostic::error("dup").with_label(Label::primary(sp(a, 7), "")),
            Diagnostic::error("dup").with_label(Label::primary(sp(a, 7), "")),
        ];
        let expected = {
            let mut s = DiagSink::new();
            for d in base.iter().cloned() {
                s.emit(d);
            }
            s.into_sorted()
        };
        // ヒープのアルゴリズムで全順列を試す (9! = 362880 は多いので、先頭 7 件の順列 + 残りの回転)。
        let n = 7;
        let mut idx: Vec<usize> = (0..base.len()).collect();
        let mut c = vec![0usize; n];
        let check = |idx: &[usize]| {
            for rot in 0..base.len() {
                let mut s = DiagSink::new();
                for k in 0..base.len() {
                    s.emit(base[idx[(k + rot) % base.len()]].clone());
                }
                assert_eq!(s.into_sorted(), expected);
            }
        };
        check(&idx);
        let mut i = 0;
        while i < n {
            if c[i] < i {
                if i % 2 == 0 {
                    idx.swap(0, i);
                } else {
                    idx.swap(c[i], i);
                }
                check(&idx);
                c[i] += 1;
                i = 0;
            } else {
                c[i] = 0;
                i += 1;
            }
        }
    }
}
