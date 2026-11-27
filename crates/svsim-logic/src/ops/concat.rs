//! 連結と複製 (IEEE 1800-2023 §11.4.12, §11.4.12.1、plan.md §3.5(e))。
//!
//! - 連結 `{a, b, …}`: 先頭の要素が MSB 側。結果の幅は各要素の幅の和。各要素は自己決定。
//! - 幅 0 の要素 (0 回の複製、§11.4.12.1) は無視する。幅 0 を許すのは連結と複製だけ (plan.md §3.2)。
//! - 複製 `{n{a}}`: `n = 0` は幅 0 の値になる (連結の中でだけ許される。その検査は sema が行う)。
//! - データを移すだけの演算なので、X/Z はそのまま運ぶ。

use crate::layout::normalize_plane;
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};
use crate::words::copy_bits;

fn check_concat_width(f: &str, dst: u32, sum: u64) {
    assert_eq!(
        dst as u64, sum,
        "{f}: dst width {dst} must equal the sum of part widths {sum}"
    );
}

fn check_replicate_width(f: &str, dst: u32, x: u32, n: u32) {
    let want = x as u64 * n as u64;
    assert_eq!(
        dst as u64, want,
        "{f}: dst width {dst} must equal {x} * {n} = {want}"
    );
}

/// `dst = {parts[0], parts[1], …}` (§11.4.12)。`parts[0]` が MSB 側。
///
/// # Panics
/// `dst` の幅が各要素の幅の和でない場合。
pub fn concat(dst: LogicMut<'_>, parts: &[LogicRef<'_>]) {
    check_concat_width(
        "concat",
        dst.width(),
        parts.iter().map(|p| p.width() as u64).sum(),
    );
    let dw = dst.width();
    let (a, b) = dst.into_planes();
    let mut off = 0usize;
    // LSB 側 (末尾の要素) から積む。
    for p in parts.iter().rev() {
        let w = p.width() as usize;
        copy_bits(a, off, p.aval(), 0, w);
        copy_bits(b, off, p.bval(), 0, w);
        off += w;
    }
    // 出力先の幅より上のビット (最上位ワードの未使用ビット) は書いていないので正規化する (plan.md §3.2)。
    normalize_plane(dw, a);
    normalize_plane(dw, b);
}

/// 2 値の [`concat()`]。
///
/// # Panics
/// `dst` の幅が各要素の幅の和でない場合。
pub fn concat2(dst: BitMut<'_>, parts: &[BitRef<'_>]) {
    check_concat_width(
        "concat2",
        dst.width(),
        parts.iter().map(|p| p.width() as u64).sum(),
    );
    let dw = dst.width();
    let a = dst.into_words();
    let mut off = 0usize;
    for p in parts.iter().rev() {
        let w = p.width() as usize;
        copy_bits(a, off, p.words(), 0, w);
        off += w;
    }
    normalize_plane(dw, a);
}

/// `dst = {n{x}}` (§11.4.12.1)。`n = 0` なら `dst` は幅 0。
///
/// # Panics
/// `dst` の幅が `x.width() * n` でない場合。
pub fn replicate(dst: LogicMut<'_>, x: LogicRef<'_>, n: u32) {
    check_replicate_width("replicate", dst.width(), x.width(), n);
    let dw = dst.width();
    let (a, b) = dst.into_planes();
    let w = x.width() as usize;
    for k in 0..n as usize {
        copy_bits(a, k * w, x.aval(), 0, w);
        copy_bits(b, k * w, x.bval(), 0, w);
    }
    normalize_plane(dw, a);
    normalize_plane(dw, b);
}

/// 2 値の [`replicate`]。
///
/// # Panics
/// `dst` の幅が `x.width() * n` でない場合。
pub fn replicate2(dst: BitMut<'_>, x: BitRef<'_>, n: u32) {
    check_replicate_width("replicate2", dst.width(), x.width(), n);
    let dw = dst.width();
    let a = dst.into_words();
    let w = x.width() as usize;
    for k in 0..n as usize {
        copy_bits(a, k * w, x.words(), 0, w);
    }
    normalize_plane(dw, a);
}

/// 幅の和 (`MAX_WIDTH` を超えれば `LogicVec::zero` が panic する)。
fn total_width(ws: impl Iterator<Item = u32>) -> u32 {
    let s: u64 = ws.map(u64::from).sum();
    u32::try_from(s).expect("concat: total width overflows u32")
}

impl LogicVec {
    /// `{parts[0], parts[1], …}` (§11.4.12)。`parts[0]` が MSB 側。
    pub fn concat(parts: &[&LogicVec]) -> LogicVec {
        let mut r = LogicVec::zero(total_width(parts.iter().map(|p| p.width())));
        // 要素数が少ない場合はビューの配列をスタックに置く (幅 ≤ 64 の結果で確保しないため)。
        const STACK: usize = 8;
        if parts.len() <= STACK {
            let empty = LogicRef::new(0, &[], &[]);
            let mut views = [empty; STACK];
            for (v, p) in views.iter_mut().zip(parts) {
                *v = p.as_ref();
            }
            concat(r.as_mut(), &views[..parts.len()]);
        } else {
            let views: Vec<LogicRef<'_>> = parts.iter().map(|p| p.as_ref()).collect();
            concat(r.as_mut(), &views);
        }
        r
    }

    /// `{n{self}}` (§11.4.12.1)。
    pub fn replicate(&self, n: u32) -> LogicVec {
        let w = u32::try_from(self.width() as u64 * n as u64)
            .expect("replicate: total width overflows u32");
        let mut r = LogicVec::zero(w);
        replicate(r.as_mut(), self.as_ref(), n);
        r
    }
}

impl BitVec {
    /// 2 値の `{parts[0], parts[1], …}` (§11.4.12)。
    pub fn concat(parts: &[&BitVec]) -> BitVec {
        let mut r = BitVec::zero(total_width(parts.iter().map(|p| p.width())));
        const STACK: usize = 8;
        if parts.len() <= STACK {
            let empty = BitRef::new(0, &[]);
            let mut views = [empty; STACK];
            for (v, p) in views.iter_mut().zip(parts) {
                *v = p.as_ref();
            }
            concat2(r.as_mut(), &views[..parts.len()]);
        } else {
            let views: Vec<BitRef<'_>> = parts.iter().map(|p| p.as_ref()).collect();
            concat2(r.as_mut(), &views);
        }
        r
    }

    /// 2 値の `{n{self}}` (§11.4.12.1)。
    pub fn replicate(&self, n: u32) -> BitVec {
        let w = u32::try_from(self.width() as u64 * n as u64)
            .expect("replicate: total width overflows u32");
        let mut r = BitVec::zero(w);
        replicate2(r.as_mut(), self.as_ref(), n);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lv(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// 出力先の幅より上のビットが汚れていても、結果は正規化される (plan.md §3.2、§11.4.12)。
    #[test]
    fn dirty_dst_is_normalized() {
        let x = lv("1x0z1");
        let y = LogicVec::from_u64(60, 0x0123_4567_89ab_cdef);
        let (mut a, mut b) = ([!0u64; 2], [!0u64; 2]);
        concat(LogicMut::new(65, &mut a, &mut b), &[x.as_ref(), y.as_ref()]);
        assert!(LogicRef::new(65, &a, &b).is_normalized());
        assert_eq!(
            LogicVec::from_ref(LogicRef::new(65, &a, &b)),
            LogicVec::concat(&[&x, &y])
        );
        let (mut a, mut b) = ([!0u64; 2], [!0u64; 2]);
        replicate(LogicMut::new(70, &mut a, &mut b), x.as_ref(), 14);
        assert!(LogicRef::new(70, &a, &b).is_normalized());
        assert_eq!(
            LogicVec::from_ref(LogicRef::new(70, &a, &b)),
            x.replicate(14)
        );
        let x2 = BitVec::from_u64(5, 0b10110);
        let mut w = [!0u64; 2];
        concat2(
            BitMut::new(70, &mut w),
            &[x2.as_ref(), BitVec::zero(65).as_ref()],
        );
        assert!(BitRef::new(70, &w).is_normalized());
        let mut w = [!0u64; 2];
        replicate2(BitMut::new(70, &mut w), x2.as_ref(), 14);
        assert!(BitRef::new(70, &w).is_normalized());
        assert_eq!(BitVec::from_ref(BitRef::new(70, &w)), x2.replicate(14));
    }

    /// §11.4.12 / §11.4.12.1: 先頭が MSB 側、幅 0 は無視、`{3{2'b1z}}`。
    #[test]
    fn concat_replicate_11_4_12() {
        let e = LogicVec::zero(0);
        let r = LogicVec::concat(&[&lv("10xz"), &e, &lv("01")]);
        assert_eq!(r.to_bits_string(), "10xz01");
        assert_eq!(LogicVec::concat(&[&e]).width(), 0);
        assert_eq!(LogicVec::concat(&[]).width(), 0);
        assert_eq!(lv("1z").replicate(3).to_bits_string(), "1z1z1z");
        assert_eq!(lv("1z").replicate(0).width(), 0);
        let parts: Vec<LogicVec> = (0..10).map(|i| LogicVec::from_u64(3, i)).collect();
        let refs: Vec<&LogicVec> = parts.iter().collect();
        let c = LogicVec::concat(&refs);
        assert_eq!(c.width(), 30);
        assert_eq!(c.as_ref().aval()[0] & 7, 9 & 7);
    }
}
