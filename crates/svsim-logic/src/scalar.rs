//! 層 1: スカラ高速パス (幅 ≤ 64) の値 [`L64`] と共通の補助関数 (plan.md §3.4)。
//!
//! 4 値は [`L64`] `{ a, b }`、2 値は `u64` で、幅 `w` (1〜64) を引数に取る。結果は
//! [`mask`]`(w)` でマスクする (正規化不変条件、plan.md §3.2)。
//!
//! **層 1 は、レジスタ上の値に対する演算の仕様であり、記憶域の配置とは独立している** (R-26)。
//! アリーナからのロード/ストアは [`L64::load`] / [`L64::store`] 等で行い、配置を変えるときは
//! そこだけを差し替える。M5 の JIT が生成する式は、この層の関数と差分テストする。
//!
//! 各演算のスカラ版は、このファイルではなく各演算のファイル (`ops/*.rs`) に置く。

use crate::logic::Logic;
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

/// 幅 `w` のマスク (下位 `w` ビットが 1)。`w >= 64` なら `!0`、`w == 0` なら 0。
///
/// [`top_mask`](crate::top_mask) と違い、`w == 64` と `w == 0` を区別する。
#[inline]
pub const fn mask(w: u32) -> u64 {
    if w >= 64 { !0 } else { (1u64 << w) - 1 }
}

/// 下位 `w` ビットを 2 の補数として 64 ビットに符号拡張する (`1 <= w <= 64`)。
///
/// 上位ビットに何が入っていても無視する。
#[inline]
pub const fn sign_extend(v: u64, w: u32) -> u64 {
    debug_assert!(w >= 1 && w <= 64);
    let sh = 64 - w;
    (((v << sh) as i64) >> sh) as u64
}

/// 下位 `w` ビットだけを残す (0 拡張)。
#[inline]
pub const fn zero_extend(v: u64, w: u32) -> u64 {
    v & mask(w)
}

/// 幅 `w` の値の符号ビット (ビット `w - 1`)。`1 <= w <= 64`。
#[inline]
pub const fn sign_bit(v: u64, w: u32) -> bool {
    debug_assert!(w >= 1 && w <= 64);
    (v >> (w - 1)) & 1 != 0
}

/// 幅 `w` の 2 値の値を拡張する。`signed` なら符号拡張、そうでなければ 0 拡張。
#[inline]
pub const fn extend(v: u64, w: u32, signed: bool) -> u64 {
    if signed {
        sign_extend(v, w)
    } else {
        zero_extend(v, w)
    }
}

/// 4 値のスカラ値 (幅 ≤ 64)。ビット `i` の値は `(a >> i) & 1` と `(b >> i) & 1` で、
/// エンコーディングは [`Logic`] と同じ (plan.md §3.1)。
///
/// 幅は値に含まない。演算は幅 `w` を引数に取る。正規化 (`w` ビット目以上が 0) は
/// [`L64::is_normalized`] で検査できる。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct L64 {
    /// aval プレーン。
    pub a: u64,
    /// bval プレーン。
    pub b: u64,
}

impl L64 {
    /// 全ビット 0。
    pub const ZERO: L64 = L64 { a: 0, b: 0 };

    /// aval と bval から作る (マスクしない)。
    #[inline]
    pub const fn new(a: u64, b: u64) -> L64 {
        L64 { a, b }
    }

    /// 2 値の値から作る (bval = 0)。幅 `w` でマスクする。
    #[inline]
    pub const fn from_u64(v: u64, w: u32) -> L64 {
        L64 {
            a: v & mask(w),
            b: 0,
        }
    }

    /// 幅 `w` の全ビットを `v` にした値。
    #[inline]
    pub const fn filled(v: Logic, w: u32) -> L64 {
        let m = mask(w);
        L64 {
            a: if v.aval() { m } else { 0 },
            b: if v.bval() { m } else { 0 },
        }
    }

    /// 幅 `w` の全 X。算術の X 規則 (§11.4.3) などで使う。
    #[inline]
    pub const fn all_x(w: u32) -> L64 {
        L64::filled(Logic::X, w)
    }

    /// ビット `i` の値 (`i < 64`)。
    ///
    /// `i >= 64` は呼び出し側の誤りで、debug ビルドでは `debug_assert!` で検出する
    /// (release ビルドではシフト量が 64 で割った余りになる)。
    #[inline]
    pub const fn bit(self, i: u32) -> Logic {
        debug_assert!(i < 64, "L64::bit: index out of range (must be < 64)");
        Logic::from_words_lsb(self.a >> i, self.b >> i)
    }

    /// X または Z のビットがあれば真。
    #[inline]
    pub const fn has_unknown(self) -> bool {
        self.b != 0
    }

    /// 幅 `w` で正規化されているか (`w` ビット目以上が両プレーンとも 0)。
    #[inline]
    pub const fn is_normalized(self, w: u32) -> bool {
        let m = mask(w);
        self.a & !m == 0 && self.b & !m == 0
    }

    /// 幅 `w` でマスクする (正規化する)。
    #[inline]
    pub const fn masked(self, w: u32) -> L64 {
        let m = mask(w);
        L64 {
            a: self.a & m,
            b: self.b & m,
        }
    }

    /// 2 値に変換する (X/Z → 0、§6.11.2)。
    #[inline]
    pub const fn to_two_state(self) -> u64 {
        self.a & !self.b
    }

    /// ビューから読み込む。
    ///
    /// # Panics
    /// `x.width() > 64` の場合。
    #[inline]
    pub fn load(x: LogicRef<'_>) -> L64 {
        assert!(x.width() <= 64, "L64::load: width {} > 64", x.width());
        match (x.aval().first(), x.bval().first()) {
            (Some(&a), Some(&b)) => L64 { a, b },
            _ => L64::ZERO,
        }
    }

    /// ビューに書き込む。`dst` の幅でマスクしてから書く。
    ///
    /// # Panics
    /// `dst.width() > 64` の場合。
    #[inline]
    pub fn store(self, dst: LogicMut<'_>) {
        let w = dst.width();
        assert!(w <= 64, "L64::store: width {w} > 64");
        let v = self.masked(w);
        let (a, b) = dst.into_planes();
        if let (Some(da), Some(db)) = (a.first_mut(), b.first_mut()) {
            *da = v.a;
            *db = v.b;
        }
    }
}

/// 2 値のビューから読み込む。
///
/// # Panics
/// `x.width() > 64` の場合。
#[inline]
pub fn load2(x: BitRef<'_>) -> u64 {
    assert!(x.width() <= 64, "load2: width {} > 64", x.width());
    x.words().first().copied().unwrap_or(0)
}

/// 2 値のビューに書き込む。`dst` の幅でマスクしてから書く。
///
/// # Panics
/// `dst.width() > 64` の場合。
#[inline]
pub fn store2(v: u64, dst: BitMut<'_>) {
    let w = dst.width();
    assert!(w <= 64, "store2: width {w} > 64");
    if let Some(d) = dst.into_words().first_mut() {
        *d = v & mask(w);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_values() {
        assert_eq!(mask(0), 0);
        assert_eq!(mask(1), 1);
        assert_eq!(mask(63), u64::MAX >> 1);
        assert_eq!(mask(64), u64::MAX);
        assert_eq!(mask(100), u64::MAX);
    }

    /// 符号拡張は符号ビットの値を複製する (§11.8.2)。
    #[test]
    fn sign_and_zero_extend() {
        assert_eq!(sign_extend(0b100, 3), !0b011);
        assert_eq!(sign_extend(0b011, 3), 0b011);
        assert_eq!(sign_extend(0xFF, 8) as i64, -1);
        assert_eq!(sign_extend(0x80, 8) as i64, -128);
        assert_eq!(sign_extend(0x7F, 8) as i64, 127);
        assert_eq!(sign_extend(1, 1) as i64, -1);
        assert_eq!(sign_extend(u64::MAX, 64), u64::MAX);
        // 上位ビットのごみは無視する。
        assert_eq!(sign_extend(0xF00 | 0x01, 8), 1);
        assert_eq!(zero_extend(0xF01, 8), 1);
        assert!(sign_bit(0x80, 8));
        assert!(!sign_bit(0x7F, 8));
        assert!(sign_bit(1 << 63, 64));
        assert_eq!(extend(0x80, 8, true), sign_extend(0x80, 8));
        assert_eq!(extend(0x180, 8, false), 0x80);
    }

    #[test]
    fn l64_constructors() {
        assert_eq!(L64::from_u64(0x1FF, 8), L64::new(0xFF, 0));
        assert_eq!(L64::filled(Logic::X, 4), L64::new(0xF, 0xF));
        assert_eq!(L64::filled(Logic::Z, 4), L64::new(0, 0xF));
        assert_eq!(L64::filled(Logic::L1, 64), L64::new(!0, 0));
        assert_eq!(L64::all_x(3), L64::new(7, 7));
        let v = L64::new(0b1100, 0b0101); // 1x0z
        assert_eq!(v.bit(0), Logic::Z);
        assert_eq!(v.bit(1), Logic::L0);
        assert_eq!(v.bit(2), Logic::X);
        assert_eq!(v.bit(3), Logic::L1);
        assert!(v.has_unknown());
        assert!(v.is_normalized(4));
        assert!(!v.is_normalized(3));
        assert_eq!(v.masked(3), L64::new(0b100, 0b101));
        // X/Z → 0 (§6.11.2)
        assert_eq!(v.to_two_state(), 0b1000);
    }

    #[test]
    fn load_store_round_trip() {
        for w in [0u32, 1, 7, 63, 64] {
            let mut words = [0u64; 2];
            let n = crate::words_for(w);
            let v = L64::new(0xDEAD_BEEF_1234_5678, 0x0F0F_0000_FFFF_0001);
            v.store(LogicMut::from_planar(w, &mut words[..2 * n]));
            let got = L64::load(LogicRef::from_planar(w, &words[..2 * n]));
            assert_eq!(got, v.masked(w), "width {w}");
            assert!(LogicRef::from_planar(w, &words[..2 * n]).is_normalized());

            let mut bw = [0u64; 1];
            store2(u64::MAX, BitMut::new(w, &mut bw[..n]));
            assert_eq!(load2(BitRef::new(w, &bw[..n])), mask(w));
        }
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "L64::bit")]
    fn bit_rejects_index_64_in_debug() {
        let _ = L64::new(!0, 0).bit(64);
    }

    #[test]
    #[should_panic(expected = "> 64")]
    fn load_rejects_wide() {
        let w = [0u64; 4];
        let _ = L64::load(LogicRef::from_planar(65, &w));
    }
}
