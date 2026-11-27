//! 条件演算子で条件が X/Z のときのマージ [`cond_merge`], [`cond_merge_bits`] (IEEE 1800-2023 §11.4.11)。
//! タスク M0-T4。
//!
//! 条件の真理値は [`truth`](crate::ops::logical::truth) で求める。真なら `t`、偽なら `f` を評価器が
//! 選び、x のときだけここのマージを呼ぶ。t と f の幅は呼び出し側で揃える (§11.6.1)。
//!
//! ビットごとに、両方 0 なら 0、両方 1 なら 1、それ以外は x (§11.4.11 の表)。
//! **z と z も x** にする (決定 R-10、規格の表を優先) [要VCS確認 V-8]。Icarus は z?z → z を残すので、
//! 既知の差として記録している。
//!
//! 混在カーネル [`cond_merge_bits`] (plan.md §3.5.1) は 2 値の t, f から 4 値の結果を直接作る。
//! 結果は 4 値のまま返し、2 値の変数に代入するときに x が 0 になる (§6.11.2)。

use crate::ops::check_same_width;
use crate::scalar::{L64, mask};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitRef, LogicMut, LogicRef};

/// 1 ワード分のマージ (plan.md §3.4 の式)。`S` = 両方が同じ既知の値のビット。
#[inline(always)]
const fn merge_w(at: u64, bt: u64, af: u64, bf: u64) -> (u64, u64) {
    let s = !(at ^ af) & !(bt ^ bf) & !bt;
    ((at & s) | !s, !s)
}

/// 2 値の 1 ワード分のマージ。
#[inline(always)]
const fn merge_bits_w(t: u64, f: u64) -> (u64, u64) {
    let d = t ^ f;
    (t | d, d)
}

// ===========================================================================
// 層 1
// ===========================================================================

/// `c ? t : f` で c が x/z のときの結果 (§11.4.11) [要VCS確認 V-8]。幅 `w` (1〜64)。
#[inline]
pub const fn cond_merge_l64(t: L64, f: L64, w: u32) -> L64 {
    let (a, b) = merge_w(t.a, t.b, f.a, f.b);
    L64::new(a, b).masked(w)
}

/// 2 値の `t`, `f` から 4 値の結果を作るマージ (§11.4.11、plan.md §3.5.1)。値が違うビットは x。
#[inline]
pub const fn cond_merge_bits_l64(t: u64, f: u64, w: u32) -> L64 {
    let (a, b) = merge_bits_w(t, f);
    let m = mask(w);
    L64::new(a & m, b & m)
}

// ===========================================================================
// 層 2
// ===========================================================================

/// `dst = (c ? t : f)` で c が x/z のときの結果 (§11.4.11) [要VCS確認 V-8]。
///
/// # Panics
/// `dst`, `t`, `f` の幅が揃っていない場合。
pub fn cond_merge(mut dst: LogicMut<'_>, t: LogicRef<'_>, f: LogicRef<'_>) {
    check_same_width("cond_merge", dst.width(), t.width());
    check_same_width("cond_merge", t.width(), f.width());
    {
        let (da, db) = dst.planes_mut();
        let it = da
            .iter_mut()
            .zip(db.iter_mut())
            .zip(t.aval().iter().zip(t.bval()))
            .zip(f.aval().iter().zip(f.bval()));
        for (((da, db), (&at, &bt)), (&af, &bf)) in it {
            let (a, b) = merge_w(at, bt, af, bf);
            *da = a;
            *db = b;
        }
    }
    dst.normalize();
}

/// 混在カーネル: 2 値の `t`, `f` から 4 値の結果を作る (§11.4.11、plan.md §3.5.1)。
/// `from_two_state` してから [`cond_merge`] を呼んだ結果と同じ。
///
/// # Panics
/// `dst`, `t`, `f` の幅が揃っていない場合。
pub fn cond_merge_bits(mut dst: LogicMut<'_>, t: BitRef<'_>, f: BitRef<'_>) {
    check_same_width("cond_merge_bits", dst.width(), t.width());
    check_same_width("cond_merge_bits", t.width(), f.width());
    {
        let (da, db) = dst.planes_mut();
        let it = da
            .iter_mut()
            .zip(db.iter_mut())
            .zip(t.words().iter().zip(f.words()));
        for ((da, db), (&tw, &fw)) in it {
            let (a, b) = merge_bits_w(tw, fw);
            *da = a;
            *db = b;
        }
    }
    dst.normalize();
}

// ===========================================================================
// 層 3
// ===========================================================================

impl LogicVec {
    /// `c ? self : f` で c が x/z のときの結果 (§11.4.11) [要VCS確認 V-8]。
    ///
    /// # Panics
    /// 幅が違う場合。
    pub fn cond_merge(&self, f: &LogicVec) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        cond_merge(r.as_mut(), self.as_ref(), f.as_ref());
        r
    }
}

impl BitVec {
    /// `c ? self : f` で c が x/z のときの結果 (§11.4.11、plan.md §3.5.1)。結果は 4 値。
    ///
    /// # Panics
    /// 幅が違う場合。
    pub fn cond_merge_logic(&self, f: &BitVec) -> LogicVec {
        let mut r = LogicVec::zero(self.width());
        cond_merge_bits(r.as_mut(), self.as_ref(), f.as_ref());
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Logic;

    fn v(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// §11.4.11 の表 (4×4)。z と z も x [要VCS確認 V-8]。
    #[test]
    fn merge_table_11_4_11() {
        for t in Logic::ALL {
            for f in Logic::ALL {
                let want = if t == f && t.is_known() { t } else { Logic::X };
                let got = cond_merge_l64(L64::filled(t, 1), L64::filled(f, 1), 1).bit(0);
                assert_eq!(got, want, "{t}:{f}");
            }
        }
        assert_eq!(v("0110").cond_merge(&v("0101")).to_bits_string(), "01xx");
        assert_eq!(v("1x0z").cond_merge(&v("1x0z")).to_bits_string(), "1x0x");
        // [要VCS確認 V-8] 暫定: 1'bx ? 1'bz : 1'bz は x (Icarus は z)
        assert_eq!(v("z").cond_merge(&v("z")).to_bits_string(), "x");
    }

    #[test]
    fn merge_bits_3_5_1() {
        let t = BitVec::from_bits_str("11001010").unwrap();
        let f = BitVec::from_bits_str("10101010").unwrap();
        assert_eq!(t.cond_merge_logic(&f).to_bits_string(), "1xx01010");
        assert_eq!(
            cond_merge_bits_l64(0b1100_1010, 0b1010_1010, 8),
            L64::load(v("1xx01010").as_ref())
        );
    }
}
