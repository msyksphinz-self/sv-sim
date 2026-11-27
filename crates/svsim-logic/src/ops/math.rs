//! `$clog2` (IEEE 1800-2023 §20.8.1、plan.md §3.5(e))。
//!
//! 引数は符号なしとして扱い、`ceil(log2(x))` を返す。`$clog2(0)` = 0、`$clog2(1)` = 0
//! [要VCS確認 V-10]。X/Z を含めば `None` で、結果をどうするか (定数式なら診断、実行時なら X) は
//! 呼び出し側が決める。幅は任意で、多倍長でも正確に計算する。

use crate::scalar::L64;
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitRef, LogicRef};

/// 2 値のワード列の `ceil(log2(x))` (`x = 0` なら 0)。
fn clog2_words(a: &[u64]) -> u32 {
    // 最上位の 1 のビット位置 p を探す。x が 2 のべきなら p、そうでなければ p + 1。
    let Some(top) = a.iter().rposition(|&w| w != 0) else {
        return 0;
    };
    let p = top as u32 * 64 + (63 - a[top].leading_zeros());
    let pow2 = a[top].is_power_of_two() && a[..top].iter().all(|&w| w == 0);
    if pow2 { p } else { p + 1 }
}

/// `$clog2(x)` (§20.8.1)。X/Z を含めば `None` [要VCS確認 V-10]。
pub fn clog2(x: LogicRef<'_>) -> Option<u32> {
    if x.has_unknown() {
        None
    } else {
        Some(clog2_words(x.aval()))
    }
}

/// 2 値の `$clog2(x)` (§20.8.1)。
pub fn clog2_2(x: BitRef<'_>) -> u32 {
    clog2_words(x.words())
}

/// 層 1 の `$clog2(x)` (幅 64 以下、正規化済み)。
#[inline]
pub const fn clog2_l64(x: L64) -> Option<u32> {
    if x.b != 0 {
        None
    } else {
        Some(clog2_2_l64(x.a))
    }
}

/// 層 1 の 2 値の `$clog2(x)`。
#[inline]
pub const fn clog2_2_l64(x: u64) -> u32 {
    if x <= 1 {
        0
    } else {
        64 - (x - 1).leading_zeros()
    }
}

impl LogicVec {
    /// `$clog2(self)` (§20.8.1)。X/Z を含めば `None`。
    pub fn clog2(&self) -> Option<u32> {
        clog2(self.as_ref())
    }
}

impl BitVec {
    /// 2 値の `$clog2(self)` (§20.8.1)。
    pub fn clog2(&self) -> u32 {
        clog2_2(self.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §20.8.1: `$clog2(0)` = 0、`$clog2(1)` = 0、2 のべきの前後。
    #[test]
    fn clog2_20_8_1() {
        assert_eq!(clog2_2_l64(0), 0);
        assert_eq!(clog2_2_l64(1), 0);
        assert_eq!(clog2_2_l64(2), 1);
        assert_eq!(clog2_2_l64(3), 2);
        assert_eq!(clog2_2_l64(5), 3);
        assert_eq!(clog2_2_l64(u64::MAX), 64);
        assert_eq!(clog2_words(&[0, 1]), 64);
        assert_eq!(clog2_words(&[1, 1]), 65);
        assert_eq!(clog2_words(&[0, 0]), 0);
        assert_eq!(clog2_l64(L64::new(1, 1)), None);
        for k in 2..63u32 {
            let p = 1u64 << k;
            assert_eq!(clog2_2_l64(p - 1), k);
            assert_eq!(clog2_2_l64(p), k);
            assert_eq!(clog2_2_l64(p + 1), k + 1);
            assert_eq!(clog2_words(&[p - 1]), k);
            assert_eq!(clog2_words(&[p]), k);
            assert_eq!(clog2_words(&[p + 1]), k + 1);
        }
    }
}
