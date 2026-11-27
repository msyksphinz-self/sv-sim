//! 作業領域 [`Scratch`] (plan.md §3.3)。
//!
//! 乗除算、べき乗、10 進書式化などのカーネルが使う一時的なワード列。呼び出し側が持ち回して
//! 使い回すことで、ホットループでのヒープ確保を避ける (層 1・層 2 はヒープ確保をしない)。
//!
//! # 受け渡しの方針
//!
//! - **公開カーネル** (層 2 の `pub fn`) は作業領域を `&mut Scratch` で受ける。
//!   必要なワード列は [`Scratch::words`] / [`Scratch::parts`] で一度に切り出す。
//! - **crate 内の補助関数** (`words.rs` や各 `ops/*.rs` の非公開関数) は `Scratch` を受けず、
//!   切り出した `&mut [u64]` を受ける。こうすると、公開カーネルの中で別の処理を入れ子に呼んでも
//!   作業領域の借用がぶつからない (例: べき乗が乗算の補助関数を呼ぶとき、`parts` で
//!   `[acc, base, tmp]` を切り出して補助関数に渡す)。
//! - 公開カーネルの中から**別の公開カーネル** (`&mut Scratch` を受けるもの) を呼ぶ必要があるときは、
//!   [`Scratch::split_words`] で自分の分のワード列と、内側のカーネルに渡す `&mut Scratch` に分ける。
//!   内側の `Scratch` は別のバッファで、初回の呼び出しで確保され、以後は使い回される。
//!   [`Scratch::reserve_for`] は最外側の段しか確保しないので、ヒープ確保 0 回 (plan.md §5.5) を
//!   保証したい経路では、[`Scratch::reserve_nested`] で内側の段も確保しておくか、公開カーネル同士の
//!   入れ子を避けて補助関数を使うこと。

/// 1 ワードあたりに [`Scratch::reserve_for`] が確保するワード数。
///
/// 目安 (plan.md §3.5(c)): `div` が `2·nw + 1`、`mul` が `nw`。べき乗は `mul` の作業領域に加えて
/// 底の写しと結果で `2·nw` 程度、10 進書式化は被除数の写しで `nw` を使う。余裕を見て `4·nw + 4`。
const WORDS_PER_WORD: usize = 4;
/// [`Scratch::reserve_for`] が加える固定のワード数。
const EXTRA_WORDS: usize = 4;

/// 演算カーネルの作業領域。
///
/// [`reserve_for`](Self::reserve_for) で事前に確保しておけば、その幅までのカーネルは
/// 確保をしない。容量が足りない場合は、カーネルの中で伸ばす (このときだけ確保が起きる)。
///
/// 内容はカーネルの呼び出しをまたいで意味を持たない。受け渡しの方針はモジュールの説明を参照。
#[derive(Default, Debug, Clone)]
pub struct Scratch {
    buf: Vec<u64>,
    /// [`split_words`](Self::split_words) が返す内側の段。初回に作る。
    inner: Option<Box<Scratch>>,
}

impl Scratch {
    /// 空の作業領域 (確保しない)。
    #[inline]
    pub const fn new() -> Self {
        Scratch {
            buf: Vec::new(),
            inner: None,
        }
    }

    /// 幅 `width` までの演算に足りる作業領域を確保して作る。
    pub fn for_width(width: u32) -> Self {
        let mut s = Scratch::new();
        s.reserve_for(width);
        s
    }

    /// 幅 `width` までの、どの M0 のカーネルにも足りるワード数 (`4·nw + 4`)。
    ///
    /// この見積もりは暫定で、乗除算・べき乗の実装 (M0-T5) で実際の必要量に合わせて見直す。
    #[inline]
    pub const fn words_needed(width: u32) -> usize {
        WORDS_PER_WORD * crate::layout::words_for(width) + EXTRA_WORDS
    }

    /// 幅 `width` までの演算に足りる容量を確保する (容量が足りていれば何もしない)。
    ///
    /// 確保するのはこの段だけで、[`split_words`](Self::split_words) の内側の段は確保しない
    /// ([`reserve_nested`](Self::reserve_nested) を参照)。
    pub fn reserve_for(&mut self, width: u32) {
        self.reserve_words(Scratch::words_needed(width));
    }

    /// この段と、[`split_words`](Self::split_words) で辿れる内側の `depth` 段のそれぞれに、
    /// 幅 `width` までの演算に足りる容量を確保する。`depth == 0` なら [`reserve_for`](Self::reserve_for) と同じ。
    pub fn reserve_nested(&mut self, width: u32, depth: usize) {
        let mut s = self;
        s.reserve_for(width);
        for _ in 0..depth {
            s = s.inner.get_or_insert_with(Box::default);
            s.reserve_for(width);
        }
    }

    /// 少なくとも `n` ワードの容量を確保する。
    pub fn reserve_words(&mut self, n: usize) {
        if self.buf.capacity() < n {
            self.buf.reserve_exact(n - self.buf.len());
        }
    }

    /// 現在の容量 (ワード数)。この段だけを数え、内側の段は含めない。
    #[inline]
    pub fn capacity_words(&self) -> usize {
        self.buf.capacity()
    }

    /// 長さ `n` の、0 で埋めたワード列を借りる。容量が足りなければ伸ばす。
    pub fn words(&mut self, n: usize) -> &mut [u64] {
        self.prepare(n);
        &mut self.buf[..n]
    }

    /// 長さ `lens[0]`, `lens[1]`, … の、互いに重ならない 0 埋めのワード列を `K` 本借りる。
    /// 容量が足りなければ伸ばす。
    ///
    /// ```
    /// # use svsim_logic::Scratch;
    /// let mut s = Scratch::for_width(128);
    /// let [q, r] = s.parts([3, 2]);
    /// assert_eq!((q.len(), r.len()), (3, 2));
    /// ```
    pub fn parts<const K: usize>(&mut self, lens: [usize; K]) -> [&mut [u64]; K] {
        let total = lens.iter().sum();
        self.prepare(total);
        let mut rest: &mut [u64] = &mut self.buf[..total];
        core::array::from_fn(|i| {
            let (head, tail) = core::mem::take(&mut rest).split_at_mut(lens[i]);
            rest = tail;
            head
        })
    }

    /// 長さ `n` の 0 埋めのワード列と、入れ子に呼ぶ公開カーネルに渡す内側の `Scratch` に分ける。
    ///
    /// 内側の `Scratch` はこの段とは別のバッファで、初回の呼び出しで作られ (このとき確保が起きる)、
    /// 以後の呼び出しでは同じものが使い回される。内側の内容は呼び出しをまたいで意味を持たない。
    ///
    /// ```
    /// # use svsim_logic::Scratch;
    /// let mut s = Scratch::for_width(128);
    /// let (mine, inner) = s.split_words(4);
    /// mine[0] = 1;
    /// let [t] = inner.parts([2]);
    /// assert_eq!((mine.len(), t.len()), (4, 2));
    /// ```
    pub fn split_words(&mut self, n: usize) -> (&mut [u64], &mut Scratch) {
        self.prepare(n);
        let inner = self.inner.get_or_insert_with(Box::default);
        (&mut self.buf[..n], inner)
    }

    /// 長さを `n` 以上にし、先頭 `n` ワードを 0 にする。
    fn prepare(&mut self, n: usize) {
        if self.buf.len() < n {
            self.buf.resize(n, 0);
        }
        self.buf[..n].fill(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_does_not_allocate() {
        let s = Scratch::new();
        assert_eq!(s.capacity_words(), 0);
    }

    #[test]
    fn reserve_for_width() {
        let s = Scratch::for_width(129);
        assert!(s.capacity_words() >= Scratch::words_needed(129));
        let div_need = 2 * 3 + 1; // div の目安 2·nw + 1 (plan.md §3.5(c))
        assert!(Scratch::words_needed(129) >= div_need);
    }

    #[test]
    fn words_are_zeroed_and_reused_without_growth() {
        let mut s = Scratch::for_width(256);
        let cap = s.capacity_words();
        {
            let w = s.words(8);
            assert!(w.iter().all(|&x| x == 0));
            w.fill(!0);
        }
        // 再度借りると 0 に戻っている。
        assert!(s.words(8).iter().all(|&x| x == 0));
        let [a, b, c] = s.parts([4, 0, 5]);
        assert_eq!((a.len(), b.len(), c.len()), (4, 0, 5));
        a.fill(1);
        c.fill(2);
        assert!(a.iter().all(|&x| x == 1));
        assert!(c.iter().all(|&x| x == 2));
        assert_eq!(s.capacity_words(), cap, "容量内なら伸ばさない");
    }

    #[test]
    fn grows_when_needed() {
        let mut s = Scratch::new();
        assert_eq!(s.words(10).len(), 10);
        assert!(s.capacity_words() >= 10);
        let [x] = s.parts([20]);
        assert_eq!(x.len(), 20);
    }

    #[test]
    fn split_words_gives_disjoint_inner_scratch() {
        let mut s = Scratch::for_width(128);
        {
            let (mine, inner) = s.split_words(3);
            assert_eq!(mine.len(), 3);
            assert!(mine.iter().all(|&x| x == 0));
            mine.fill(7);
            // 内側の段は別のバッファなので、mine を書き換えても影響しない。
            let w = inner.words(5);
            assert!(w.iter().all(|&x| x == 0));
            w.fill(9);
            assert!(mine.iter().all(|&x| x == 7));
        }
        // 2 回目は内側の段を使い回す (容量が残っている)。
        let (mine, inner) = s.split_words(3);
        assert!(mine.iter().all(|&x| x == 0), "借りるたびに 0 に戻る");
        assert!(inner.capacity_words() >= 5);
        // 内側の段もさらに分けられる。
        let (m2, inner2) = inner.split_words(2);
        assert_eq!(m2.len(), 2);
        assert_eq!(inner2.words(1).len(), 1);
    }

    #[test]
    fn reserve_nested_reserves_inner_levels() {
        let mut s = Scratch::new();
        s.reserve_nested(256, 2);
        let need = Scratch::words_needed(256);
        assert!(s.capacity_words() >= need);
        let (_, i1) = s.split_words(1);
        assert!(i1.capacity_words() >= need);
        let (_, i2) = i1.split_words(1);
        assert!(i2.capacity_words() >= need);
        let mut t = Scratch::new();
        t.reserve_nested(64, 0);
        assert!(t.capacity_words() >= Scratch::words_needed(64));
        assert!(t.inner.is_none());
    }
}
