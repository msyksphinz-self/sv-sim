//! リダクション演算 (IEEE 1800-2023 §11.4.9) と `$countbits` 系 (§20.9)。タスク M0-T4。
//!
//! オペランドは自己決定 (幅はそのまま) で、結果は 1 ビット ([`Logic`]) か数。
//!
//! - AND 系: 既知の 0 が 1 つでもあれば 0、無くて未知 (x/z) があれば x、それ以外は 1。
//! - OR 系: 既知の 1 があれば 1、無くて未知があれば x、それ以外は 0。
//! - XOR 系: 未知があれば x、無ければパリティ。
//! - 否定形 (`~& ~| ~^`) は、上の結果に 1 ビットの `~` を適用したもの (x は x のまま)。
//!
//! | 層 | 4 値 | 2 値 |
//! |---|---|---|
//! | 層 1 | `reduce_*_l64`, [`count_bits_l64`] ほか | `reduce_*2_l64`, [`count_bits2_l64`] ほか |
//! | 層 2 | `reduce_*`, [`count_bits`] ほか | `reduce_*2`, [`count_bits2`] ほか |
//! | 層 3 | `LogicVec::reduce_*` ほか | `BitVec::reduce_*` ほか |

use crate::layout::word_mask;
use crate::logic::Logic;
use crate::ops::check_nonzero_width;
use crate::scalar::{L64, mask};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitRef, LogicRef};

// ===========================================================================
// LogicSet ($countbits の制御ビットの集合)
// ===========================================================================

/// `$countbits(e, ...)` の制御ビット (0/1/x/z) の集合 (§20.9)。
///
/// 内部は [`Logic`] の repr (0..=3) ごとに 1 ビットを持つビットマスク。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct LogicSet(u8);

impl LogicSet {
    /// 空の集合。`count_bits` は 0 を返す (決定 Q-2)。
    pub const EMPTY: LogicSet = LogicSet(0);
    /// `{'1}` (`$countones` と同じ)。
    pub const ONE: LogicSet = LogicSet::EMPTY.with(Logic::L1);
    /// `{'0}`。
    pub const ZERO: LogicSet = LogicSet::EMPTY.with(Logic::L0);
    /// `{'x, 'z}` (`$isunknown` と同じ)。
    pub const UNKNOWN: LogicSet = LogicSet::EMPTY.with(Logic::X).with(Logic::Z);
    /// 4 値すべて。
    pub const ALL: LogicSet = LogicSet(0b1111);

    /// 値の並びから作る (重複してよい)。
    pub const fn of(values: &[Logic]) -> LogicSet {
        let mut s = LogicSet::EMPTY;
        let mut i = 0;
        while i < values.len() {
            s = s.with(values[i]);
            i += 1;
        }
        s
    }

    /// `v` を加えた集合。
    #[inline]
    pub const fn with(self, v: Logic) -> LogicSet {
        LogicSet(self.0 | (1 << v as u8))
    }

    /// `v` を含むか。
    #[inline]
    pub const fn contains(self, v: Logic) -> bool {
        self.0 & (1 << v as u8) != 0
    }

    /// 空か。
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

// ===========================================================================
// ワード単位の集計 (層 1 と層 2 で共有する)
// ===========================================================================

/// 1 ワード分の各値のビット数 `[n0, n1, nz, nx]` (Logic の repr 順)。`m` は有効ビットのマスク。
#[inline(always)]
const fn counts_w(a: u64, b: u64, m: u64) -> [u32; 4] {
    [
        (!a & !b & m).count_ones(),
        (a & !b).count_ones(),
        (!a & b).count_ones(),
        (a & b).count_ones(),
    ]
}

#[inline(always)]
const fn sum_set(c: [u32; 4], set: LogicSet) -> u32 {
    let mut n = 0;
    let mut i = 0;
    while i < 4 {
        if set.0 & (1 << i) != 0 {
            n += c[i];
        }
        i += 1;
    }
    n
}

// ===========================================================================
// 層 1: 4 値
// ===========================================================================

/// 単項 `&x` (§11.4.9)。幅 `w` (1〜64)。
#[inline]
pub const fn reduce_and_l64(x: L64, w: u32) -> Logic {
    let m = mask(w);
    if !x.a & !x.b & m != 0 {
        Logic::L0
    } else if x.b != 0 {
        Logic::X
    } else {
        Logic::L1
    }
}

/// 単項 `~&x` (§11.4.9)。
#[inline]
pub const fn reduce_nand_l64(x: L64, w: u32) -> Logic {
    reduce_and_l64(x, w).not()
}

/// 単項 `|x` (§11.4.9)。
#[inline]
pub const fn reduce_or_l64(x: L64, w: u32) -> Logic {
    let _ = w;
    if x.a & !x.b != 0 {
        Logic::L1
    } else if x.b != 0 {
        Logic::X
    } else {
        Logic::L0
    }
}

/// 単項 `~|x` (§11.4.9)。
#[inline]
pub const fn reduce_nor_l64(x: L64, w: u32) -> Logic {
    reduce_or_l64(x, w).not()
}

/// 単項 `^x` (§11.4.9)。
#[inline]
pub const fn reduce_xor_l64(x: L64, w: u32) -> Logic {
    let _ = w;
    if x.b != 0 {
        Logic::X
    } else if x.a.count_ones() & 1 != 0 {
        Logic::L1
    } else {
        Logic::L0
    }
}

/// 単項 `~^x` / `^~x` (§11.4.9)。
#[inline]
pub const fn reduce_xnor_l64(x: L64, w: u32) -> Logic {
    reduce_xor_l64(x, w).not()
}

/// `$countbits(x, set)` (§20.9)。`set` に含まれる値を持つビットの数。
#[inline]
pub const fn count_bits_l64(x: L64, w: u32, set: LogicSet) -> u32 {
    sum_set(counts_w(x.a, x.b, mask(w)), set)
}

/// `$countones(x)` (§20.9)。
#[inline]
pub const fn count_ones_l64(x: L64, w: u32) -> u32 {
    let _ = w;
    (x.a & !x.b).count_ones()
}

/// `$onehot(x)` (§20.9) = `$countbits(x, '1) == 1`。
#[inline]
pub const fn onehot_l64(x: L64, w: u32) -> bool {
    count_ones_l64(x, w) == 1
}

/// `$onehot0(x)` (§20.9) = `$countbits(x, '1) <= 1`。
#[inline]
pub const fn onehot0_l64(x: L64, w: u32) -> bool {
    count_ones_l64(x, w) <= 1
}

/// `$isunknown(x)` (§20.9) = `$countbits(x, 'x, 'z) != 0`。
#[inline]
pub const fn is_unknown_l64(x: L64, w: u32) -> bool {
    let _ = w;
    x.b != 0
}

// ===========================================================================
// 層 1: 2 値
// ===========================================================================

/// 2 値の単項 `&x` (§11.4.9)。
#[inline]
pub const fn reduce_and2_l64(x: u64, w: u32) -> bool {
    x == mask(w)
}

/// 2 値の単項 `~&x` (§11.4.9)。
#[inline]
pub const fn reduce_nand2_l64(x: u64, w: u32) -> bool {
    !reduce_and2_l64(x, w)
}

/// 2 値の単項 `|x` (§11.4.9)。
#[inline]
pub const fn reduce_or2_l64(x: u64, w: u32) -> bool {
    let _ = w;
    x != 0
}

/// 2 値の単項 `~|x` (§11.4.9)。
#[inline]
pub const fn reduce_nor2_l64(x: u64, w: u32) -> bool {
    !reduce_or2_l64(x, w)
}

/// 2 値の単項 `^x` (§11.4.9)。
#[inline]
pub const fn reduce_xor2_l64(x: u64, w: u32) -> bool {
    let _ = w;
    x.count_ones() & 1 != 0
}

/// 2 値の単項 `~^x` (§11.4.9)。
#[inline]
pub const fn reduce_xnor2_l64(x: u64, w: u32) -> bool {
    !reduce_xor2_l64(x, w)
}

/// 2 値の `$countbits(x, set)` (§20.9)。x/z は数えるビットが無い。
#[inline]
pub const fn count_bits2_l64(x: u64, w: u32, set: LogicSet) -> u32 {
    sum_set(counts_w(x, 0, mask(w)), set)
}

/// 2 値の `$countones(x)` (§20.9)。
#[inline]
pub const fn count_ones2_l64(x: u64, w: u32) -> u32 {
    let _ = w;
    x.count_ones()
}

/// 2 値の `$onehot(x)` (§20.9)。
#[inline]
pub const fn onehot2_l64(x: u64, w: u32) -> bool {
    count_ones2_l64(x, w) == 1
}

/// 2 値の `$onehot0(x)` (§20.9)。
#[inline]
pub const fn onehot02_l64(x: u64, w: u32) -> bool {
    count_ones2_l64(x, w) <= 1
}

// ===========================================================================
// 層 2: 4 値
// ===========================================================================

/// 単項 `&x` (§11.4.9)。
pub fn reduce_and(x: LogicRef<'_>) -> Logic {
    check_nonzero_width("reduce_and", x.width());
    let w = x.width();
    let mut unknown = false;
    for (i, (&a, &b)) in x.aval().iter().zip(x.bval()).enumerate() {
        if !a & !b & word_mask(w, i) != 0 {
            return Logic::L0;
        }
        unknown |= b != 0;
    }
    if unknown { Logic::X } else { Logic::L1 }
}

/// 単項 `~&x` (§11.4.9)。
pub fn reduce_nand(x: LogicRef<'_>) -> Logic {
    reduce_and(x).not()
}

/// 単項 `|x` (§11.4.9)。
pub fn reduce_or(x: LogicRef<'_>) -> Logic {
    check_nonzero_width("reduce_or", x.width());
    let mut unknown = false;
    for (&a, &b) in x.aval().iter().zip(x.bval()) {
        if a & !b != 0 {
            return Logic::L1;
        }
        unknown |= b != 0;
    }
    if unknown { Logic::X } else { Logic::L0 }
}

/// 単項 `~|x` (§11.4.9)。
pub fn reduce_nor(x: LogicRef<'_>) -> Logic {
    reduce_or(x).not()
}

/// 単項 `^x` (§11.4.9)。
pub fn reduce_xor(x: LogicRef<'_>) -> Logic {
    check_nonzero_width("reduce_xor", x.width());
    if x.has_unknown() {
        return Logic::X;
    }
    let p = x.aval().iter().fold(0u64, |acc, &a| acc ^ a);
    Logic::from(p.count_ones() & 1 != 0)
}

/// 単項 `~^x` / `^~x` (§11.4.9)。
pub fn reduce_xnor(x: LogicRef<'_>) -> Logic {
    reduce_xor(x).not()
}

/// `$countbits(x, set)` (§20.9)。`set` に含まれる値を持つビットの数。空の集合なら 0 (決定 Q-2)。
pub fn count_bits(x: LogicRef<'_>, set: LogicSet) -> u32 {
    check_nonzero_width("count_bits", x.width());
    let w = x.width();
    x.aval()
        .iter()
        .zip(x.bval())
        .enumerate()
        .map(|(i, (&a, &b))| sum_set(counts_w(a, b, word_mask(w, i)), set))
        .sum()
}

/// `$countones(x)` (§20.9) = `$countbits(x, '1)`。
pub fn count_ones(x: LogicRef<'_>) -> u32 {
    check_nonzero_width("count_ones", x.width());
    x.aval()
        .iter()
        .zip(x.bval())
        .map(|(&a, &b)| (a & !b).count_ones())
        .sum()
}

/// `$onehot(x)` (§20.9) = `$countbits(x, '1) == 1`。
pub fn onehot(x: LogicRef<'_>) -> bool {
    count_ones(x) == 1
}

/// `$onehot0(x)` (§20.9) = `$countbits(x, '1) <= 1`。
pub fn onehot0(x: LogicRef<'_>) -> bool {
    count_ones(x) <= 1
}

/// `$isunknown(x)` (§20.9) = `$countbits(x, 'x, 'z) != 0`。
pub fn is_unknown(x: LogicRef<'_>) -> bool {
    check_nonzero_width("is_unknown", x.width());
    x.has_unknown()
}

// ===========================================================================
// 層 2: 2 値
// ===========================================================================

/// 2 値の単項 `&x` (§11.4.9)。
pub fn reduce_and2(x: BitRef<'_>) -> bool {
    check_nonzero_width("reduce_and2", x.width());
    let w = x.width();
    x.words()
        .iter()
        .enumerate()
        .all(|(i, &a)| a == word_mask(w, i))
}

/// 2 値の単項 `~&x` (§11.4.9)。
pub fn reduce_nand2(x: BitRef<'_>) -> bool {
    !reduce_and2(x)
}

/// 2 値の単項 `|x` (§11.4.9)。
pub fn reduce_or2(x: BitRef<'_>) -> bool {
    check_nonzero_width("reduce_or2", x.width());
    x.words().iter().any(|&a| a != 0)
}

/// 2 値の単項 `~|x` (§11.4.9)。
pub fn reduce_nor2(x: BitRef<'_>) -> bool {
    !reduce_or2(x)
}

/// 2 値の単項 `^x` (§11.4.9)。
pub fn reduce_xor2(x: BitRef<'_>) -> bool {
    check_nonzero_width("reduce_xor2", x.width());
    x.words().iter().fold(0u64, |acc, &a| acc ^ a).count_ones() & 1 != 0
}

/// 2 値の単項 `~^x` (§11.4.9)。
pub fn reduce_xnor2(x: BitRef<'_>) -> bool {
    !reduce_xor2(x)
}

/// 2 値の `$countbits(x, set)` (§20.9)。
pub fn count_bits2(x: BitRef<'_>, set: LogicSet) -> u32 {
    check_nonzero_width("count_bits2", x.width());
    let ones = count_ones2(x);
    let mut n = 0;
    if set.contains(Logic::L1) {
        n += ones;
    }
    if set.contains(Logic::L0) {
        n += x.width() - ones;
    }
    n
}

/// 2 値の `$countones(x)` (§20.9)。
pub fn count_ones2(x: BitRef<'_>) -> u32 {
    check_nonzero_width("count_ones2", x.width());
    x.words().iter().map(|a| a.count_ones()).sum()
}

/// 2 値の `$onehot(x)` (§20.9)。
pub fn onehot2(x: BitRef<'_>) -> bool {
    count_ones2(x) == 1
}

/// 2 値の `$onehot0(x)` (§20.9)。
pub fn onehot02(x: BitRef<'_>) -> bool {
    count_ones2(x) <= 1
}

// ===========================================================================
// 層 3
// ===========================================================================

impl LogicVec {
    /// 単項 `&self` (§11.4.9)。
    pub fn reduce_and(&self) -> Logic {
        reduce_and(self.as_ref())
    }
    /// 単項 `~&self` (§11.4.9)。
    pub fn reduce_nand(&self) -> Logic {
        reduce_nand(self.as_ref())
    }
    /// 単項 `|self` (§11.4.9)。
    pub fn reduce_or(&self) -> Logic {
        reduce_or(self.as_ref())
    }
    /// 単項 `~|self` (§11.4.9)。
    pub fn reduce_nor(&self) -> Logic {
        reduce_nor(self.as_ref())
    }
    /// 単項 `^self` (§11.4.9)。
    pub fn reduce_xor(&self) -> Logic {
        reduce_xor(self.as_ref())
    }
    /// 単項 `~^self` (§11.4.9)。
    pub fn reduce_xnor(&self) -> Logic {
        reduce_xnor(self.as_ref())
    }
    /// `$countbits(self, set)` (§20.9)。
    pub fn count_bits(&self, set: LogicSet) -> u32 {
        count_bits(self.as_ref(), set)
    }
    /// `$countones(self)` (§20.9)。
    pub fn count_ones(&self) -> u32 {
        count_ones(self.as_ref())
    }
    /// `$onehot(self)` (§20.9)。
    pub fn onehot(&self) -> bool {
        onehot(self.as_ref())
    }
    /// `$onehot0(self)` (§20.9)。
    pub fn onehot0(&self) -> bool {
        onehot0(self.as_ref())
    }
    /// `$isunknown(self)` (§20.9)。
    pub fn is_unknown(&self) -> bool {
        is_unknown(self.as_ref())
    }
}

impl BitVec {
    /// 2 値の単項 `&self` (§11.4.9)。
    pub fn reduce_and(&self) -> bool {
        reduce_and2(self.as_ref())
    }
    /// 2 値の単項 `~&self` (§11.4.9)。
    pub fn reduce_nand(&self) -> bool {
        reduce_nand2(self.as_ref())
    }
    /// 2 値の単項 `|self` (§11.4.9)。
    pub fn reduce_or(&self) -> bool {
        reduce_or2(self.as_ref())
    }
    /// 2 値の単項 `~|self` (§11.4.9)。
    pub fn reduce_nor(&self) -> bool {
        reduce_nor2(self.as_ref())
    }
    /// 2 値の単項 `^self` (§11.4.9)。
    pub fn reduce_xor(&self) -> bool {
        reduce_xor2(self.as_ref())
    }
    /// 2 値の単項 `~^self` (§11.4.9)。
    pub fn reduce_xnor(&self) -> bool {
        reduce_xnor2(self.as_ref())
    }
    /// 2 値の `$countbits(self, set)` (§20.9)。
    pub fn count_bits(&self, set: LogicSet) -> u32 {
        count_bits2(self.as_ref(), set)
    }
    /// 2 値の `$countones(self)` (§20.9)。
    pub fn count_ones(&self) -> u32 {
        count_ones2(self.as_ref())
    }
    /// 2 値の `$onehot(self)` (§20.9)。
    pub fn onehot(&self) -> bool {
        onehot2(self.as_ref())
    }
    /// 2 値の `$onehot0(self)` (§20.9)。
    pub fn onehot0(&self) -> bool {
        onehot02(self.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// §11.4.9 のリダクションの例。
    #[test]
    fn reductions_11_4_9() {
        use Logic::{L0 as O, L1 as I, X};
        let cases = [
            ("1111", [I, O, I, O, O, I]),
            ("0000", [O, I, O, I, O, I]),
            ("1011", [O, I, I, O, I, O]),
            ("1x11", [X, X, I, O, X, X]),
            ("0x11", [O, I, I, O, X, X]),
            ("000z", [O, I, X, X, X, X]),
        ];
        for (s, want) in cases {
            let x = v(s);
            let got = [
                x.reduce_and(),
                x.reduce_nand(),
                x.reduce_or(),
                x.reduce_nor(),
                x.reduce_xor(),
                x.reduce_xnor(),
            ];
            assert_eq!(got, want, "{s}");
            let l = L64::load(x.as_ref());
            let got1 = [
                reduce_and_l64(l, 4),
                reduce_nand_l64(l, 4),
                reduce_or_l64(l, 4),
                reduce_nor_l64(l, 4),
                reduce_xor_l64(l, 4),
                reduce_xnor_l64(l, 4),
            ];
            assert_eq!(got1, want, "l64 {s}");
        }
    }

    /// 最上位ワードの余りのビットを既知の 0 と見なさない。
    #[test]
    fn reduce_and_ignores_padding() {
        assert_eq!(LogicVec::filled(65, Logic::L1).reduce_and(), Logic::L1);
        assert_eq!(LogicVec::filled(3, Logic::L1).reduce_and(), Logic::L1);
        assert!(BitVec::filled(129, true).reduce_and());
        assert_eq!(
            LogicVec::filled(65, Logic::L0).count_bits(LogicSet::ZERO),
            65
        );
    }

    /// §20.9 の `$countbits` 系。
    #[test]
    fn countbits_20_9() {
        let x = v("10xz01xz");
        assert_eq!(x.count_bits(LogicSet::ONE), 2);
        assert_eq!(x.count_bits(LogicSet::ZERO), 2);
        assert_eq!(x.count_bits(LogicSet::of(&[Logic::X])), 2);
        assert_eq!(x.count_bits(LogicSet::of(&[Logic::Z])), 2);
        assert_eq!(x.count_bits(LogicSet::UNKNOWN), 4);
        assert_eq!(x.count_bits(LogicSet::ALL), 8);
        assert_eq!(x.count_bits(LogicSet::EMPTY), 0);
        assert_eq!(x.count_ones(), 2);
        assert!(v("0001x000").onehot());
        assert!(!v("00010001").onehot());
        assert!(v("00000000").onehot0());
        assert!(v("0000000z").is_unknown());
        assert_eq!(count_bits2_l64(0b0110, 4, LogicSet::ZERO), 2);
    }
}
