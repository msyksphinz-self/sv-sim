//! 作業領域 [`Scratch`] (plan.md §3.3)。
//!
//! 乗除算、べき乗、10 進書式化などのカーネルが使う一時的なワード列。呼び出し側が持ち回して
//! 使い回すことで、ホットループでのヒープ確保を避ける (層 1・層 2 はヒープ確保をしない)。

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
/// 内容はカーネルの呼び出しをまたいで意味を持たない。
#[derive(Default, Debug, Clone)]
pub struct Scratch {
    buf: Vec<u64>,
}

impl Scratch {
    /// 空の作業領域 (確保しない)。
    #[inline]
    pub const fn new() -> Self {
        Scratch { buf: Vec::new() }
    }

    /// 幅 `width` までの演算に足りる作業領域を確保して作る。
    pub fn for_width(width: u32) -> Self {
        let mut s = Scratch::new();
        s.reserve_for(width);
        s
    }

    /// 幅 `width` までの、どの M0 のカーネルにも足りるワード数。
    #[inline]
    pub const fn words_needed(width: u32) -> usize {
        WORDS_PER_WORD * crate::layout::words_for(width) + EXTRA_WORDS
    }

    /// 幅 `width` までの演算に足りる容量を確保する (容量が足りていれば何もしない)。
    pub fn reserve_for(&mut self, width: u32) {
        self.reserve_words(Scratch::words_needed(width));
    }

    /// 少なくとも `n` ワードの容量を確保する。
    pub fn reserve_words(&mut self, n: usize) {
        if self.buf.capacity() < n {
            self.buf.reserve_exact(n - self.buf.len());
        }
    }

    /// 現在の容量 (ワード数)。
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
}
