//! ソースマップ: ファイル ID、span、行・列の計算 (plan.md §4.1、タスク M0-T10)。
//!
//! # 方針
//! - ソーステキストは**バイト列**で持つ。SystemVerilog のソースが UTF-8 とは限らず、
//!   字句解析もバイト単位で行うため。表示するときだけ損失のある変換をする。
//! - 1 ファイルの上限は 4 GiB (`u32` のオフセット)。[`Span`] は 12 バイト。
//! - 行の区切りは LF (`\n`) だけ。CRLF の `\r` は行のテキストから取り除く
//!   ([`SourceMap::line_text`])。単独の CR は行の区切りとして扱わない。
//!   (IEEE 1800-2023 §5.3 は改行を空白の一種として扱うだけで、行の区切りの
//!   バイト列を規定しない。ここでの規則は表示用の約束である。)
//! - 行・列はどちらも 1 始まり。列は行頭からの UTF-8 の文字数 + 1。
//!   不正な UTF-8 のバイトは 1 バイトを 1 文字として数える。

use std::path::PathBuf;

/// [`SourceMap`] に登録したファイルの ID。登録順に 0, 1, 2, … が振られる。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct FileId(u32);

impl FileId {
    /// ファイル表の添字 (0 始まり)。
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// バイトオフセットの半開区間 `[lo, hi)`。
///
/// 順序 (`Ord`) は (ファイル ID、開始位置、終了位置) の辞書式。
/// 診断を決定的な順序に並べるのに使う (plan.md §4.2)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Span {
    /// ファイル。
    pub file: FileId,
    /// 開始バイトオフセット (含む)。
    pub lo: u32,
    /// 終了バイトオフセット (含まない)。
    pub hi: u32,
}

impl Span {
    /// `[lo, hi)` の span を作る。`lo <= hi` であること (デバッグビルドで検査する)。
    pub fn new(file: FileId, lo: u32, hi: u32) -> Self {
        debug_assert!(lo <= hi, "Span::new: lo ({lo}) > hi ({hi})");
        Span { file, lo, hi }
    }

    /// バイト長。
    pub fn len(self) -> u32 {
        self.hi.saturating_sub(self.lo)
    }

    /// 長さ 0 の span (位置だけを指す) なら `true`。
    pub fn is_empty(self) -> bool {
        self.hi <= self.lo
    }
}

/// ファイルの出どころ。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum FileOrigin {
    /// ディスク上のファイル。
    Disk(PathBuf),
    /// 仮想ファイル (コマンドラインの define など)。
    Virtual {
        /// 表示に使う名前。
        name: String,
    },
    /// `` `include `` で取り込んだファイル (M1 で使う。IEEE 1800-2023 §22.4)。
    Include {
        /// 取り込んだファイルのパス。
        path: PathBuf,
        /// `` `include `` 指令の位置。
        from: Span,
    },
    /// マクロ展開の結果を仮想ファイルとして持つ (M1 で使う。IEEE 1800-2023 §22.5.1)。
    MacroExpansion {
        /// マクロを使った位置。
        call_site: Span,
        /// マクロを定義した位置。
        def_site: Span,
    },
}

/// 1 行・1 列から数える位置。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct LineCol {
    /// 行番号 (1 始まり)。
    pub line: u32,
    /// 列番号 (1 始まり、UTF-8 の文字単位。不正なバイトは 1 バイト = 1 文字)。
    pub col: u32,
}

/// 登録済みのソースファイル。
#[derive(Clone, Debug)]
pub struct SourceFile {
    origin: FileOrigin,
    text: Box<[u8]>,
    /// 各行の先頭のバイトオフセット。常に 1 個以上あり、先頭は 0。
    line_starts: Vec<u32>,
}

impl SourceFile {
    fn new(origin: FileOrigin, text: Vec<u8>) -> Self {
        assert!(
            u32::try_from(text.len()).is_ok(),
            "source file larger than 4 GiB is not supported (plan.md §4.1)"
        );
        let mut line_starts = vec![0u32];
        line_starts.extend(
            text.iter()
                .enumerate()
                .filter(|&(_, &b)| b == b'\n')
                .map(|(i, _)| (i + 1) as u32),
        );
        SourceFile {
            origin,
            text: text.into_boxed_slice(),
            line_starts,
        }
    }

    /// ファイルの出どころ。
    pub fn origin(&self) -> &FileOrigin {
        &self.origin
    }

    /// ファイルの内容 (バイト列)。
    pub fn text(&self) -> &[u8] {
        &self.text
    }

    /// バイト長。
    pub fn len(&self) -> u32 {
        self.text.len() as u32
    }

    /// 空のファイルなら `true`。
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// 行数。空のファイルは 1 行 (空の行) と数える。最後が改行で終わるファイルは、
    /// その後ろに空の行が 1 つあるものとして数える。
    pub fn line_count(&self) -> u32 {
        self.line_starts.len() as u32
    }

    /// 表示に使う名前。ディスク上のファイルと `` `include `` はパス、
    /// 仮想ファイルはその名前、マクロ展開は `<macro expansion>`。
    pub fn display_name(&self) -> String {
        match &self.origin {
            FileOrigin::Disk(p) => p.display().to_string(),
            FileOrigin::Include { path, .. } => path.display().to_string(),
            FileOrigin::Virtual { name } => name.clone(),
            FileOrigin::MacroExpansion { .. } => "<macro expansion>".to_string(),
        }
    }

    /// `offset` の行・列。`offset` がファイル長を超えるときはファイル末尾として扱う。
    ///
    /// `offset` が複数バイトの文字の途中を指すときは、その文字の途中までのバイトを
    /// 不正なバイトとして 1 バイトずつ数える。
    pub fn line_col(&self, offset: u32) -> LineCol {
        let offset = offset.min(self.len());
        // line_starts[0] == 0 なので、partition_point は 1 以上。
        let idx = self.line_starts.partition_point(|&s| s <= offset) - 1;
        let start = self.line_starts[idx] as usize;
        let col = count_chars(&self.text[start..offset as usize]) + 1;
        LineCol {
            line: idx as u32 + 1,
            col,
        }
    }

    /// `line` 行目 (1 始まり) のテキスト。行末の `\n` と、その直前の `\r` は含まない。
    ///
    /// # Panics
    /// `line` が 0 か、[`SourceFile::line_count`] を超えるとき。
    pub fn line_text(&self, line: u32) -> &[u8] {
        assert!(
            line >= 1 && line <= self.line_count(),
            "line_text: line {line} out of range (1..={})",
            self.line_count()
        );
        let idx = (line - 1) as usize;
        let start = self.line_starts[idx] as usize;
        let mut end = match self.line_starts.get(idx + 1) {
            Some(&next) => next as usize - 1, // `\n` の位置
            None => self.text.len(),
        };
        if end > start && self.text[end - 1] == b'\r' && end < self.text.len() {
            // CRLF の `\r` を除く。最終行 (改行なし) の末尾の単独の `\r` は残す。
            end -= 1;
        }
        &self.text[start..end]
    }

    /// `line` 行目 (1 始まり) の先頭のバイトオフセット。
    ///
    /// # Panics
    /// `line` が 0 か、[`SourceFile::line_count`] を超えるとき。
    pub fn line_start(&self, line: u32) -> u32 {
        assert!(
            line >= 1 && line <= self.line_count(),
            "line_start: line {line} out of range (1..={})",
            self.line_count()
        );
        self.line_starts[(line - 1) as usize]
    }
}

/// UTF-8 の文字数を数える。不正なバイトは 1 バイトを 1 文字とする。
fn count_chars(bytes: &[u8]) -> u32 {
    bytes
        .utf8_chunks()
        .map(|c| c.valid().chars().count() + c.invalid().len())
        .sum::<usize>() as u32
}

/// 全ソースファイルの表。
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    /// 空のソースマップ。
    pub fn new() -> Self {
        Self::default()
    }

    /// ファイルを登録して ID を返す。
    ///
    /// # Panics
    /// `text` が 4 GiB 以上のとき、またはファイル数が `u32` を超えるとき。
    pub fn add_file(&mut self, origin: FileOrigin, text: Vec<u8>) -> FileId {
        let id = FileId(u32::try_from(self.files.len()).expect("too many source files"));
        self.files.push(SourceFile::new(origin, text));
        id
    }

    /// 登録済みのファイル数。
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// ファイルが 1 つも無ければ `true`。
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// ファイルを取り出す。
    ///
    /// # Panics
    /// `id` がこのソースマップのものでないとき。
    pub fn file(&self, id: FileId) -> &SourceFile {
        &self.files[id.index()]
    }

    /// `file` の `offset` の行・列 (1 始まり)。詳細は [`SourceFile::line_col`]。
    pub fn line_col(&self, file: FileId, offset: u32) -> LineCol {
        self.file(file).line_col(offset)
    }

    /// `file` の `line` 行目 (1 始まり) のテキスト。詳細は [`SourceFile::line_text`]。
    pub fn line_text(&self, file: FileId, line: u32) -> &[u8] {
        self.file(file).line_text(line)
    }

    /// `span` が指すバイト列。
    ///
    /// # Panics
    /// `span` がファイルの範囲外か、`lo > hi` のとき。
    pub fn snippet(&self, span: Span) -> &[u8] {
        &self.file(span.file).text[span.lo as usize..span.hi as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_with(text: &[u8]) -> (SourceMap, FileId) {
        let mut sm = SourceMap::new();
        let id = sm.add_file(
            FileOrigin::Virtual {
                name: "t.sv".into(),
            },
            text.to_vec(),
        );
        (sm, id)
    }

    fn lc(line: u32, col: u32) -> LineCol {
        LineCol { line, col }
    }

    #[test]
    fn span_is_12_bytes() {
        assert_eq!(std::mem::size_of::<Span>(), 12);
    }

    #[test]
    fn file_ids_are_sequential() {
        let mut sm = SourceMap::new();
        assert!(sm.is_empty());
        let a = sm.add_file(FileOrigin::Disk("a.sv".into()), b"a".to_vec());
        let b = sm.add_file(FileOrigin::Disk("b.sv".into()), b"b".to_vec());
        assert_eq!(a.index(), 0);
        assert_eq!(b.index(), 1);
        assert!(a < b);
        assert_eq!(sm.len(), 2);
        assert_eq!(sm.file(b).text(), b"b");
        assert_eq!(sm.file(a).display_name(), "a.sv");
    }

    #[test]
    fn lf_lines() {
        let (sm, f) = map_with(b"ab\ncd\n");
        assert_eq!(sm.file(f).line_count(), 3);
        assert_eq!(sm.line_col(f, 0), lc(1, 1));
        assert_eq!(sm.line_col(f, 1), lc(1, 2));
        assert_eq!(sm.line_col(f, 2), lc(1, 3)); // `\n` 自身は前の行
        assert_eq!(sm.line_col(f, 3), lc(2, 1));
        assert_eq!(sm.line_col(f, 5), lc(2, 3));
        assert_eq!(sm.line_col(f, 6), lc(3, 1)); // EOF (改行の後ろの空行)
        assert_eq!(sm.line_text(f, 1), b"ab");
        assert_eq!(sm.line_text(f, 2), b"cd");
        assert_eq!(sm.line_text(f, 3), b"");
    }

    #[test]
    fn crlf_lines() {
        let (sm, f) = map_with(b"ab\r\ncd\r\nef");
        assert_eq!(sm.file(f).line_count(), 3);
        assert_eq!(sm.line_col(f, 4), lc(2, 1));
        assert_eq!(sm.line_col(f, 9), lc(3, 2));
        assert_eq!(sm.line_text(f, 1), b"ab");
        assert_eq!(sm.line_text(f, 2), b"cd");
        assert_eq!(sm.line_text(f, 3), b"ef");
        assert_eq!(sm.file(f).line_start(3), 8);
    }

    #[test]
    fn lone_cr_is_not_a_line_break() {
        let (sm, f) = map_with(b"a\rb\nc\r");
        assert_eq!(sm.file(f).line_count(), 2);
        assert_eq!(sm.line_col(f, 2), lc(1, 3));
        assert_eq!(sm.line_text(f, 1), b"a\rb");
        // 最終行の末尾の単独の CR は CRLF ではないので残す。
        assert_eq!(sm.line_text(f, 2), b"c\r");
    }

    #[test]
    fn no_trailing_newline() {
        let (sm, f) = map_with(b"line1\nlast");
        assert_eq!(sm.file(f).line_count(), 2);
        assert_eq!(sm.line_col(f, 10), lc(2, 5)); // EOF
        assert_eq!(sm.line_text(f, 2), b"last");
    }

    #[test]
    fn empty_file() {
        let (sm, f) = map_with(b"");
        assert!(sm.file(f).is_empty());
        assert_eq!(sm.file(f).line_count(), 1);
        assert_eq!(sm.line_col(f, 0), lc(1, 1));
        assert_eq!(sm.line_text(f, 1), b"");
        // 範囲外のオフセットは末尾として扱う。
        assert_eq!(sm.line_col(f, 100), lc(1, 1));
    }

    #[test]
    fn utf8_columns_count_chars() {
        // "é" は 2 バイト、"日" は 3 バイト。
        let text = "aé日b".as_bytes();
        let (sm, f) = map_with(text);
        assert_eq!(sm.line_col(f, 1), lc(1, 2));
        assert_eq!(sm.line_col(f, 3), lc(1, 3));
        assert_eq!(sm.line_col(f, 6), lc(1, 4));
        assert_eq!(sm.line_col(f, 7), lc(1, 5));
        // 文字の途中を指すと、途中までのバイトを 1 バイト 1 文字で数える。
        assert_eq!(sm.line_col(f, 2), lc(1, 3));
    }

    #[test]
    fn invalid_utf8_counts_bytes() {
        let (sm, f) = map_with(b"a\xff\xfeb\n\xc3x");
        assert_eq!(sm.line_col(f, 3), lc(1, 4));
        assert_eq!(sm.line_col(f, 4), lc(1, 5));
        assert_eq!(sm.line_col(f, 6), lc(2, 2)); // 不完全な \xc3 は 1 文字
        assert_eq!(sm.line_col(f, 7), lc(2, 3));
        assert_eq!(sm.line_text(f, 1), b"a\xff\xfeb");
    }

    #[test]
    fn snippet_and_span() {
        let (sm, f) = map_with(b"module m;\nendmodule\n");
        let s = Span::new(f, 7, 8);
        assert_eq!(sm.snippet(s), b"m");
        assert_eq!(s.len(), 1);
        assert!(!s.is_empty());
        assert!(Span::new(f, 3, 3).is_empty());
        assert_eq!(sm.snippet(Span::new(f, 10, 19)), b"endmodule");
    }

    #[test]
    fn span_order_is_file_lo_hi() {
        let mut sm = SourceMap::new();
        let a = sm.add_file(FileOrigin::Disk("a".into()), vec![0; 10]);
        let b = sm.add_file(FileOrigin::Disk("b".into()), vec![0; 10]);
        let mut v = vec![
            Span::new(b, 0, 1),
            Span::new(a, 3, 5),
            Span::new(a, 3, 4),
            Span::new(a, 1, 9),
        ];
        v.sort();
        assert_eq!(
            v,
            vec![
                Span::new(a, 1, 9),
                Span::new(a, 3, 4),
                Span::new(a, 3, 5),
                Span::new(b, 0, 1),
            ]
        );
    }

    #[test]
    fn display_names() {
        let mut sm = SourceMap::new();
        let d = sm.add_file(FileOrigin::Disk("dir/x.sv".into()), vec![]);
        let i = sm.add_file(
            FileOrigin::Include {
                path: "inc.svh".into(),
                from: Span::new(d, 0, 0),
            },
            vec![],
        );
        let m = sm.add_file(
            FileOrigin::MacroExpansion {
                call_site: Span::new(d, 0, 0),
                def_site: Span::new(d, 0, 0),
            },
            vec![],
        );
        assert_eq!(sm.file(d).display_name(), "dir/x.sv");
        assert_eq!(sm.file(i).display_name(), "inc.svh");
        assert_eq!(sm.file(m).display_name(), "<macro expansion>");
    }

    #[test]
    #[should_panic]
    fn line_text_out_of_range_panics() {
        let (sm, f) = map_with(b"a\n");
        let _ = sm.line_text(f, 3);
    }
}
