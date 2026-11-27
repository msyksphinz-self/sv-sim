//! 真理値 `truth` と論理演算 `! && || -> <->` (IEEE 1800-2023 §11.4.7, §12.4)。タスク M0-T4。
//!
//! - 式の真理値 ([`truth`]): 既知の 1 があれば 1、全ビットが既知の 0 なら 0、それ以外は x。
//!   `if (e)`、`?:` の条件、論理演算のオペランドで使う (§12.4, §11.4.7, §11.4.11)。
//! - 論理演算は、各オペランド (自己決定) の真理値に 1 ビットの演算 [`Logic::and`] などを適用する。
//!   `&&` は AND の表、`||` は OR の表 (§11.4.8 と同じ)。`a -> b` = `!a || b`、
//!   `a <-> b` = `(a -> b) && (b -> a)`。
//! - 短絡評価 (§11.3.5) は評価器の責務。ここでは両オペランドの真理値を受け取るだけ。
//!
//! 1 ビットの演算 (`Logic::{not, and, or, implies, equiv}`) もこのファイルに置く
//! (同じ crate 内なら `impl` ブロックを分けられる。plan.md §3.3)。

use crate::logic::Logic;
use crate::ops::check_nonzero_width;
use crate::scalar::L64;
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitRef, LogicRef};

// ===========================================================================
// 1 ビットの演算 (Logic)
// ===========================================================================

impl Logic {
    /// 1 ビットの `~` / `!` (§11.4.8, §11.4.7)。0→1、1→0、x/z→x。
    #[inline]
    #[allow(clippy::should_implement_trait)] // const fn にするため (JIT の式の仕様、plan.md §3.4)
    pub const fn not(self) -> Logic {
        match self {
            Logic::L0 => Logic::L1,
            Logic::L1 => Logic::L0,
            Logic::X | Logic::Z => Logic::X,
        }
    }

    /// 1 ビットの AND (§11.4.8 の表。`&&` も同じ表、§11.4.7)。0 が勝つ。
    #[inline]
    pub const fn and(self, other: Logic) -> Logic {
        match (self, other) {
            (Logic::L0, _) | (_, Logic::L0) => Logic::L0,
            (Logic::L1, Logic::L1) => Logic::L1,
            _ => Logic::X,
        }
    }

    /// 1 ビットの OR (§11.4.8 の表。`||` も同じ表、§11.4.7)。1 が勝つ。
    #[inline]
    pub const fn or(self, other: Logic) -> Logic {
        match (self, other) {
            (Logic::L1, _) | (_, Logic::L1) => Logic::L1,
            (Logic::L0, Logic::L0) => Logic::L0,
            _ => Logic::X,
        }
    }

    /// 論理含意 `self -> other` = `!self || other` (§11.4.7)。
    #[inline]
    pub const fn implies(self, other: Logic) -> Logic {
        self.not().or(other)
    }

    /// 論理同値 `self <-> other` = `(self -> other) && (other -> self)` (§11.4.7)。
    #[inline]
    pub const fn equiv(self, other: Logic) -> Logic {
        self.implies(other).and(other.implies(self))
    }
}

// ===========================================================================
// 層 1
// ===========================================================================

/// 式の真理値 (§11.4.7, §12.4)。幅 `w` (1〜64)。
#[inline]
pub const fn truth_l64(x: L64, w: u32) -> Logic {
    let _ = w;
    if x.a & !x.b != 0 {
        Logic::L1
    } else if x.b == 0 {
        Logic::L0
    } else {
        Logic::X
    }
}

/// `!x` (§11.4.7) = `truth(x).not()`。
#[inline]
pub const fn logical_not_l64(x: L64, w: u32) -> Logic {
    truth_l64(x, w).not()
}

/// 2 値の真理値 (§12.4)。0 以外なら真。
#[inline]
pub const fn truth2_l64(x: u64, w: u32) -> bool {
    let _ = w;
    x != 0
}

/// 2 値の `!x` (§11.4.7)。
#[inline]
pub const fn logical_not2_l64(x: u64, w: u32) -> bool {
    !truth2_l64(x, w)
}

// ===========================================================================
// 層 2
// ===========================================================================

/// 式の真理値 (§11.4.7, §12.4)。既知の 1 があれば 1、全ビットが既知の 0 なら 0、それ以外は x。
pub fn truth(x: LogicRef<'_>) -> Logic {
    check_nonzero_width("truth", x.width());
    let mut unknown = false;
    for (&a, &b) in x.aval().iter().zip(x.bval()) {
        if a & !b != 0 {
            return Logic::L1;
        }
        unknown |= b != 0;
    }
    if unknown { Logic::X } else { Logic::L0 }
}

/// `!x` (§11.4.7) = `truth(x).not()`。
pub fn logical_not(x: LogicRef<'_>) -> Logic {
    truth(x).not()
}

/// 2 値の真理値 (§12.4)。0 以外なら真。
pub fn truth2(x: BitRef<'_>) -> bool {
    check_nonzero_width("truth2", x.width());
    x.words().iter().any(|&a| a != 0)
}

/// 2 値の `!x` (§11.4.7)。
pub fn logical_not2(x: BitRef<'_>) -> bool {
    !truth2(x)
}

// ===========================================================================
// 層 3
// ===========================================================================

impl LogicVec {
    /// 真理値 (§11.4.7, §12.4)。
    pub fn truth(&self) -> Logic {
        truth(self.as_ref())
    }

    /// `!self` (§11.4.7)。
    pub fn logical_not(&self) -> Logic {
        logical_not(self.as_ref())
    }
}

impl BitVec {
    /// 2 値の真理値 (§12.4)。
    pub fn truth(&self) -> bool {
        truth2(self.as_ref())
    }

    /// 2 値の `!self` (§11.4.7)。
    pub fn logical_not(&self) -> bool {
        logical_not2(self.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Logic::{L0 as O, L1 as I, X, Z};

    /// §11.4.7 の論理演算の表 (オペランドは真理値 0/1/x)。
    #[test]
    fn logical_table_11_4_7() {
        // (a, b, &&, ||, ->, <->)
        let t = [
            (O, O, O, O, I, I),
            (O, I, O, I, I, O),
            (O, X, O, X, I, X),
            (I, O, O, I, O, O),
            (I, I, I, I, I, I),
            (I, X, X, I, X, X),
            (X, O, O, X, X, X),
            (X, I, X, I, I, X),
            (X, X, X, X, X, X),
        ];
        for (a, b, and, or, imp, eqv) in t {
            assert_eq!(a.and(b), and, "{a}&&{b}");
            assert_eq!(a.or(b), or, "{a}||{b}");
            assert_eq!(a.implies(b), imp, "{a}->{b}");
            assert_eq!(a.equiv(b), eqv, "{a}<->{b}");
        }
        assert_eq!(Z.not(), X);
        assert_eq!(Z.and(O), O);
        assert_eq!(Z.or(I), I);
    }

    /// §12.4 の真理値。
    #[test]
    fn truth_12_4() {
        let t = |s: &str| LogicVec::from_bits_str(s).unwrap().truth();
        assert_eq!(t("0000"), O);
        assert_eq!(t("0100"), I);
        assert_eq!(t("000x"), X);
        assert_eq!(t("0z00"), X);
        assert_eq!(t("100x"), I);
        assert_eq!(LogicVec::from_bits_str("000x").unwrap().logical_not(), X);
        let mut w = LogicVec::zero(129);
        w.set_bit(128, I);
        assert_eq!(w.truth(), I);
        assert_eq!(truth_l64(L64::new(0b10, 0b01), 2), I);
        assert!(BitVec::from_u64(8, 4).truth());
        assert!(BitVec::zero(8).logical_not());
    }
}
