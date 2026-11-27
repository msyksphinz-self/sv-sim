//! シミュレーション状態の `WordArena` (plan.md §4.4 (2)、タスク M0-T11)。
//!
//! PLAN §1 原則 1「全シミュレーション状態はフラットなアリーナに置く」の基礎。
//! 状態は `u64` ワードの連続した列 1 本で持つ。これでスナップショットは memcpy で済み、
//! JIT やパーティションからのアクセスもオフセットだけで済む。
//!
//! # 使い方
//! 1. [`ArenaLayoutBuilder::alloc`] で各値の領域 (ワード数と位置合わせ) を決める。
//! 2. [`ArenaLayoutBuilder::finish`] で [`ArenaLayout`] を得る。
//! 3. [`WordArena::new`] で 0 初期化したアリーナを作り、[`WordArena::slice`] などで読み書きする。
//!
//! # アリーナが知らないこと
//! アリーナは「何がどこにあるか」(信号名 → オフセット、幅、2 値/4 値の別) を知らない。
//! その対応表は M1 のエラボ/MIR が持つ。0 で初期化した値は、4 値でも「0」を意味する
//! (`logic` の初期値 X (IEEE 1800-2023 §6.8) はエラボ側が明示的に書く)。
//!
//! # 上限
//! オフセットは [`WordOffset`] (`u32`、ワード単位)。アリーナの長さも `u32` ワードに収める
//! ため、最大 `u32::MAX` ワード (2^32 ワード = 32 GiB 弱) まで。溢れは
//! [`ArenaError::Overflow`] で報告する (plan-review R-23)。
//!
//! # 決定性
//! [`WordArena::content_hash`] は、各ワードをリトルエンディアンの 8 バイトとして並べた
//! バイト列に FNV-1a 64 をかけた値。プラットフォーム (エンディアン、ポインタ幅) に依存しない。

use std::fmt;

/// アリーナ内のワード (`u64`) 単位のオフセット。
///
/// 表せるのは 2^32 ワード = 32 GiB までの範囲 (plan-review R-23)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WordOffset(pub u32);

impl WordOffset {
    /// `usize` の添字にする。
    #[inline]
    pub fn index(self) -> usize {
        self.0 as usize
    }
}

/// アリーナ操作の誤り。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ArenaError {
    /// `align_words` が 0、または 2 のべきでない (plan.md Q-15)。
    BadAlign,
    /// オフセットまたは長さが `u32` ワードを溢れた (plan.md Q-15、R-23)。
    Overflow,
    /// 区間がアリーナの範囲外 (`off + len > アリーナの長さ`)。
    OutOfRange,
    /// [`WordArena::disjoint_mut`] の区間どうしが重なっている。
    Overlap,
}

impl fmt::Display for ArenaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            ArenaError::BadAlign => "alignment must be a non-zero power of two",
            ArenaError::Overflow => "arena offset overflows u32 words (32 GiB limit)",
            ArenaError::OutOfRange => "range is outside the arena",
            ArenaError::Overlap => "ranges overlap",
        };
        f.write_str(msg)
    }
}

impl std::error::Error for ArenaError {}

/// アリーナの配置を決める builder。先頭から順に領域を割り当てる。
#[derive(Clone, Debug, Default)]
pub struct ArenaLayoutBuilder {
    next: u32,
}

impl ArenaLayoutBuilder {
    /// 空の builder を作る。
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// `words` ワードの領域を、オフセットが `align_words` の倍数になる位置に割り当てる。
    ///
    /// - `align_words` は 0 でない 2 のべき (1, 2, 8 …)。そうでなければ
    ///   `Err(ArenaError::BadAlign)` (Q-15)。
    /// - 位置合わせ後のオフセット、または末尾 (`offset + words`) が `u32` を溢れたら
    ///   `Err(ArenaError::Overflow)` (Q-15、R-23)。失敗したときは builder の状態を変えない。
    /// - `words == 0` は `align_words` の検査 (`BadAlign`) だけを行い、位置合わせせずに
    ///   現在位置をそのまま返す。次の位置も進めない (領域を消費しない。Q-15、マネージャ決定)。
    ///   幅 0 の値 (plan.md §3.2) のため。返るオフセットは常にレイアウトの長さ以下なので、
    ///   長さ 0 の `slice` / `disjoint_mut` にそのまま使える。
    ///
    /// 位置合わせはアリーナの先頭からの相対値。ベースアドレスのキャッシュライン境界への
    /// 位置合わせは M4/M6 で行う。
    pub fn alloc(&mut self, words: u32, align_words: u32) -> Result<WordOffset, ArenaError> {
        if !align_words.is_power_of_two() {
            return Err(ArenaError::BadAlign);
        }
        if words == 0 {
            return Ok(WordOffset(self.next));
        }
        let off = self
            .next
            .checked_next_multiple_of(align_words)
            .ok_or(ArenaError::Overflow)?;
        self.next = off.checked_add(words).ok_or(ArenaError::Overflow)?;
        Ok(WordOffset(off))
    }

    /// これまでに割り当てた長さ (ワード数)。
    #[inline]
    pub fn len_words(&self) -> u32 {
        self.next
    }

    /// 配置を確定する。
    #[inline]
    pub fn finish(self) -> ArenaLayout {
        ArenaLayout {
            len_words: self.next,
        }
    }
}

/// 確定したアリーナの配置。M0 では長さだけを持つ。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ArenaLayout {
    len_words: u32,
}

impl ArenaLayout {
    /// アリーナの長さ (ワード数)。
    #[inline]
    pub fn len_words(&self) -> u32 {
        self.len_words
    }
}

/// FNV-1a 64 の offset basis。
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 の prime。
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// 各ワードをリトルエンディアンの 8 バイトとして並べた列の FNV-1a 64。
fn fnv1a_words(words: &[u64]) -> u64 {
    let mut h = FNV_OFFSET_BASIS;
    for w in words {
        for byte in w.to_le_bytes() {
            h ^= u64::from(byte);
            h = h.wrapping_mul(FNV_PRIME);
        }
    }
    h
}

/// 範囲 `[off, off + len)` を検査して `usize` の区間にする。
#[inline]
fn checked_range(
    arena_len: usize,
    off: WordOffset,
    len: u32,
) -> Result<(usize, usize), ArenaError> {
    let lo = off.index();
    let hi = lo.checked_add(len as usize).ok_or(ArenaError::OutOfRange)?;
    if hi > arena_len {
        return Err(ArenaError::OutOfRange);
    }
    Ok((lo, hi))
}

/// シミュレーション状態のアリーナ。`u64` ワードの連続した列で、作った時点では全部 0。
#[derive(Clone, PartialEq, Eq)]
pub struct WordArena {
    words: Vec<u64>,
}

impl fmt::Debug for WordArena {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WordArena")
            .field("len_words", &self.words.len())
            .field(
                "content_hash",
                &format_args!("{:#018x}", self.content_hash()),
            )
            .finish()
    }
}

impl WordArena {
    /// `layout` の長さの、0 で初期化したアリーナを作る。
    pub fn new(layout: &ArenaLayout) -> Self {
        WordArena {
            words: vec![0; layout.len_words as usize],
        }
    }

    /// 長さ (ワード数)。
    #[inline]
    pub fn len_words(&self) -> usize {
        self.words.len()
    }

    /// 全ワード。
    #[inline]
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// 全ワード (可変)。
    #[inline]
    pub fn words_mut(&mut self) -> &mut [u64] {
        &mut self.words
    }

    /// `[off, off + len)` のワード列。
    ///
    /// # Panics
    /// 区間がアリーナの範囲外の場合 (`len == 0` でも `off <= 長さ` が必要。Q-16 と同じ規則)。
    #[inline]
    pub fn slice(&self, off: WordOffset, len: u32) -> &[u64] {
        match checked_range(self.words.len(), off, len) {
            Ok((lo, hi)) => &self.words[lo..hi],
            Err(_) => panic!(
                "WordArena::slice: range [{}, +{}) is outside the arena (len {})",
                off.0,
                len,
                self.words.len()
            ),
        }
    }

    /// `[off, off + len)` のワード列 (可変)。
    ///
    /// # Panics
    /// 区間がアリーナの範囲外の場合 (`len == 0` でも `off <= 長さ` が必要)。
    #[inline]
    pub fn slice_mut(&mut self, off: WordOffset, len: u32) -> &mut [u64] {
        let n = self.words.len();
        match checked_range(n, off, len) {
            Ok((lo, hi)) => &mut self.words[lo..hi],
            Err(_) => panic!(
                "WordArena::slice_mut: range [{}, +{}) is outside the arena (len {})",
                off.0, len, n
            ),
        }
    }

    /// 互いに重ならない複数の可変スライスを同時に取る (dst と src を同時に取るため)。
    ///
    /// 戻り値の `k` 番目は `ranges[k]` に対応する (渡した順。オフセット順ではない)。
    ///
    /// - どれかの区間が範囲外 → `Err(ArenaError::OutOfRange)`。
    /// - 長さが 0 でない区間どうしが重なる → `Err(ArenaError::Overlap)`。
    /// - 長さ 0 の区間は空集合なので、どの区間とも重ならない扱い。ただし `off <= 長さ`
    ///   (範囲内) であること (Q-16)。
    ///
    /// 範囲外の検査を重なりの検査より先に行う。ヒープ確保はしない。
    pub fn disjoint_mut<const N: usize>(
        &mut self,
        ranges: [(WordOffset, u32); N],
    ) -> Result<[&mut [u64]; N], ArenaError> {
        let n = self.words.len();
        let mut spans = [(0usize, 0usize); N];
        for (span, &(off, len)) in spans.iter_mut().zip(ranges.iter()) {
            *span = checked_range(n, off, len)?;
        }
        for i in 0..N {
            let (lo_i, hi_i) = spans[i];
            if lo_i == hi_i {
                continue;
            }
            for &(lo_j, hi_j) in &spans[i + 1..] {
                if lo_j != hi_j && lo_i < hi_j && lo_j < hi_i {
                    return Err(ArenaError::Overlap);
                }
            }
        }

        // 長さ 0 でない区間をオフセット順に並べ、先頭から split_at_mut で切り出す。
        let mut order = [0usize; N];
        for (k, o) in order.iter_mut().enumerate() {
            *o = k;
        }
        order.sort_unstable_by_key(|&k| spans[k].0);

        let mut out: [Option<&mut [u64]>; N] = [const { None }; N];
        let mut rest: &mut [u64] = &mut self.words;
        let mut cursor = 0usize;
        for &k in &order {
            let (lo, hi) = spans[k];
            if lo == hi {
                out[k] = Some(&mut []);
                continue;
            }
            // 重なりが無いので lo >= cursor。
            let (_, tail) = std::mem::take(&mut rest).split_at_mut(lo - cursor);
            let (piece, tail) = tail.split_at_mut(hi - lo);
            out[k] = Some(piece);
            rest = tail;
            cursor = hi;
        }
        Ok(out.map(|s| s.expect("disjoint_mut: every range is assigned")))
    }

    /// 全体のコピーを取る (memcpy)。
    pub fn snapshot(&self) -> ArenaSnapshot {
        ArenaSnapshot {
            words: self.words.clone().into_boxed_slice(),
        }
    }

    /// 既存のスナップショットのバッファに全体をコピーする (確保を再利用する)。
    ///
    /// # Panics
    /// `s` の長さがアリーナと違う場合。
    pub fn snapshot_into(&self, s: &mut ArenaSnapshot) {
        assert_eq!(
            s.words.len(),
            self.words.len(),
            "WordArena::snapshot_into: snapshot length differs from arena length"
        );
        s.words.copy_from_slice(&self.words);
    }

    /// スナップショットの内容に戻す。`s` は同じ layout から作ったアリーナのものに限る。
    ///
    /// # Panics
    /// `s` の長さがアリーナと違う場合 (呼び出し側の誤り。Q-17)。panic したときアリーナは
    /// 変更しない。
    pub fn restore(&mut self, s: &ArenaSnapshot) {
        assert_eq!(
            s.words.len(),
            self.words.len(),
            "WordArena::restore: snapshot length differs from arena length"
        );
        self.words.copy_from_slice(&s.words);
    }

    /// 決定性テスト用の内容ハッシュ。
    ///
    /// 各ワードをリトルエンディアンの 8 バイトとして並べた列に FNV-1a 64
    /// (offset basis `0xcbf29ce484222325`、prime `0x100000001b3`) をかける (R-23)。
    /// プラットフォームに依存しない。長さ 0 なら offset basis そのもの。
    pub fn content_hash(&self) -> u64 {
        fnv1a_words(&self.words)
    }
}

/// [`WordArena`] の全体のコピー。
#[derive(Clone, PartialEq, Eq)]
pub struct ArenaSnapshot {
    words: Box<[u64]>,
}

impl ArenaSnapshot {
    /// 長さ (ワード数)。
    #[inline]
    pub fn len_words(&self) -> usize {
        self.words.len()
    }

    /// 保存した全ワード。
    #[inline]
    pub fn words(&self) -> &[u64] {
        &self.words
    }

    /// 保存した内容の [`WordArena::content_hash`] と同じ定義のハッシュ。
    pub fn content_hash(&self) -> u64 {
        fnv1a_words(&self.words)
    }
}

impl fmt::Debug for ArenaSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArenaSnapshot")
            .field("len_words", &self.words.len())
            .field(
                "content_hash",
                &format_args!("{:#018x}", self.content_hash()),
            )
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena_of(words: &[u64]) -> WordArena {
        let mut b = ArenaLayoutBuilder::new();
        b.alloc(words.len() as u32, 1).unwrap();
        let mut a = WordArena::new(&b.finish());
        a.words_mut().copy_from_slice(words);
        a
    }

    #[test]
    fn fnv_known_values() {
        // 空入力は offset basis。[0] は 0x00 が 8 バイト (test-plan TC70 の固定値)。
        assert_eq!(fnv1a_words(&[]), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a_words(&[0]), 0xa8c7_f832_281a_39c5);
    }

    #[test]
    fn alloc_failure_keeps_state() {
        let mut b = ArenaLayoutBuilder::new();
        b.alloc(5, 1).unwrap();
        assert_eq!(b.alloc(u32::MAX, 1), Err(ArenaError::Overflow));
        assert_eq!(b.alloc(1, 3), Err(ArenaError::BadAlign));
        assert_eq!(b.len_words(), 5);
        assert_eq!(b.alloc(1, 1), Ok(WordOffset(5)));
    }

    #[test]
    fn disjoint_mut_returns_in_request_order() {
        let mut a = arena_of(&[0, 1, 2, 3, 4, 5]);
        let [x, y, z] = a
            .disjoint_mut([(WordOffset(4), 2), (WordOffset(0), 1), (WordOffset(2), 0)])
            .unwrap();
        assert_eq!(x, &[4, 5]);
        assert_eq!(y, &[0]);
        assert!(z.is_empty());
        x[0] = 40;
        y[0] = 100;
        assert_eq!(a.words(), &[100, 1, 2, 3, 40, 5]);
    }

    #[test]
    fn snapshot_into_reuses_buffer() {
        let mut a = arena_of(&[1, 2, 3]);
        let mut s = a.snapshot();
        a.words_mut()[1] = 20;
        a.snapshot_into(&mut s);
        a.words_mut()[1] = 0;
        a.restore(&s);
        assert_eq!(a.words(), &[1, 20, 3]);
        assert_eq!(s.content_hash(), a.content_hash());
    }

    #[test]
    #[should_panic(expected = "outside the arena")]
    fn slice_out_of_range_panics() {
        let a = arena_of(&[0; 4]);
        let _ = a.slice(WordOffset(3), 2);
    }

    #[test]
    fn slice_zero_len_at_end() {
        let mut a = arena_of(&[0; 4]);
        assert!(a.slice(WordOffset(4), 0).is_empty());
        assert!(a.slice_mut(WordOffset(4), 0).is_empty());
    }
}
