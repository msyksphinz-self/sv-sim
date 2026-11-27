//! 等価・比較・ワイルドカード等価・casez/casex の照合 (IEEE 1800-2023 §11.4.4〜§11.4.6, §12.5.1)。
//! タスク M0-T4。
//!
//! オペランドは呼び出し側で同じ幅 (max 幅) に拡張しておく (§11.6.1、plan.md §3.6)。結果は 1 ビット。
//!
//! | 演算 | 規則 | 条 |
//! |---|---|---|
//! | `==` `!=` ([`eq`], [`ne`]) | 既知ビットに不一致があれば 0 (`!=` は 1)。無くて未知があれば x。全ビット既知で一致すれば 1 [要VCS確認 V-15] | §11.4.5 |
//! | `===` `!==` ([`case_eq`], [`case_ne`]) | aval・bval とも完全に一致すれば真 | §11.4.5 |
//! | `==?` `!=?` ([`wildcard_eq`], [`wildcard_ne`]) | **右**の x/z はワイルドカード。既知の不一致があれば 0、左に未知があれば x、それ以外は 1 [要VCS確認 V-16] | §11.4.6 |
//! | casez / casex ([`casez_match`], [`casex_match`]) | casez: どちらかが z のビットは無視し、残りを `===` で比べる。casex: どちらかが x/z のビットは無視する [要VCS確認 V-17] | §12.5.1 |
//! | `< <= > >=` ([`lt`] ほか) | どちらかに未知があれば x。`signed` (両オペランドが符号付き、§11.8.1) のときだけ符号付きで比べる | §11.4.4 |
//!
//! 2 値版は名前の末尾に `2` を付け、`bool` を返す。層 1 は `_l64` 接尾辞。

use std::cmp::Ordering;

use crate::logic::Logic;
use crate::ops::bitwise::check_same_width;
use crate::scalar::{L64, sign_extend};
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitRef, LogicRef};

// ===========================================================================
// ワード単位の補助
// ===========================================================================

/// 既知の値 (2 値のワード列) の大小比較。幅 `w` で正規化されていること。
/// `signed` なら 2 の補数として比べる。
fn cmp_words(x: &[u64], y: &[u64], w: u32, signed: bool) -> Ordering {
    if w == 0 {
        return Ordering::Equal;
    }
    if signed {
        let top = ((w - 1) / 64) as usize;
        let sh = (w - 1) % 64;
        let sx = (x[top] >> sh) & 1 != 0;
        let sy = (y[top] >> sh) & 1 != 0;
        if sx != sy {
            // 負のほうが小さい。符号が同じなら、2 の補数の順序は符号なしの順序と一致する。
            return if sx {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }
    }
    for (a, b) in x.iter().rev().zip(y.iter().rev()) {
        match a.cmp(b) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

/// `==` の判定 (ワード列)。`(known_mismatch, any_unknown)` の畳み込み。
#[inline(always)]
fn eq_result(mismatch: bool, unknown: bool) -> Logic {
    if mismatch {
        Logic::L0
    } else if unknown {
        Logic::X
    } else {
        Logic::L1
    }
}

// ===========================================================================
// 層 1: 4 値
// ===========================================================================

/// `x == y` (§11.4.5) [要VCS確認 V-15]。
#[inline]
pub const fn eq_l64(x: L64, y: L64, w: u32) -> Logic {
    let _ = w;
    let unk = x.b | y.b;
    if (x.a ^ y.a) & !unk != 0 {
        Logic::L0
    } else if unk != 0 {
        Logic::X
    } else {
        Logic::L1
    }
}

/// `x != y` (§11.4.5)。
#[inline]
pub const fn ne_l64(x: L64, y: L64, w: u32) -> Logic {
    eq_l64(x, y, w).not()
}

/// `x === y` (§11.4.5)。
#[inline]
pub const fn case_eq_l64(x: L64, y: L64, w: u32) -> bool {
    let _ = w;
    x.a == y.a && x.b == y.b
}

/// `x !== y` (§11.4.5)。
#[inline]
pub const fn case_ne_l64(x: L64, y: L64, w: u32) -> bool {
    !case_eq_l64(x, y, w)
}

/// `x ==? y` (§11.4.6) [要VCS確認 V-16]。右の x/z はワイルドカード。
#[inline]
pub const fn wildcard_eq_l64(x: L64, y: L64, w: u32) -> Logic {
    let _ = w;
    let care = !y.b;
    if (x.a ^ y.a) & care & !x.b != 0 {
        Logic::L0
    } else if x.b & care != 0 {
        Logic::X
    } else {
        Logic::L1
    }
}

/// `x !=? y` (§11.4.6)。
#[inline]
pub const fn wildcard_ne_l64(x: L64, y: L64, w: u32) -> Logic {
    wildcard_eq_l64(x, y, w).not()
}

/// `casez` の照合 (§12.5.1) [要VCS確認 V-17]。どちらかが z のビットは無視する。
#[inline]
pub const fn casez_match_l64(x: L64, y: L64, w: u32) -> bool {
    let _ = w;
    let ignore = (x.b & !x.a) | (y.b & !y.a);
    ((x.a ^ y.a) | (x.b ^ y.b)) & !ignore == 0
}

/// `casex` の照合 (§12.5.1) [要VCS確認 V-17]。どちらかが x/z のビットは無視する。
#[inline]
pub const fn casex_match_l64(x: L64, y: L64, w: u32) -> bool {
    let _ = w;
    (x.a ^ y.a) & !(x.b | y.b) == 0
}

/// 関係演算の共通部分 (層 1)。未知があれば `None`。
#[inline]
const fn cmp_l64(x: L64, y: L64, w: u32, signed: bool) -> Option<Ordering> {
    if (x.b | y.b) != 0 {
        return None;
    }
    Some(cmp2_l64(x.a, y.a, w, signed))
}

#[inline]
const fn ord_to_logic(o: Option<Ordering>, lt: bool, eq: bool, gt: bool) -> Logic {
    match o {
        None => Logic::X,
        Some(Ordering::Less) => Logic::from_ab(lt, false),
        Some(Ordering::Equal) => Logic::from_ab(eq, false),
        Some(Ordering::Greater) => Logic::from_ab(gt, false),
    }
}

/// `x < y` (§11.4.4)。どちらかに未知があれば x。
#[inline]
pub const fn lt_l64(x: L64, y: L64, w: u32, signed: bool) -> Logic {
    ord_to_logic(cmp_l64(x, y, w, signed), true, false, false)
}

/// `x <= y` (§11.4.4)。
#[inline]
pub const fn le_l64(x: L64, y: L64, w: u32, signed: bool) -> Logic {
    ord_to_logic(cmp_l64(x, y, w, signed), true, true, false)
}

/// `x > y` (§11.4.4)。
#[inline]
pub const fn gt_l64(x: L64, y: L64, w: u32, signed: bool) -> Logic {
    ord_to_logic(cmp_l64(x, y, w, signed), false, false, true)
}

/// `x >= y` (§11.4.4)。
#[inline]
pub const fn ge_l64(x: L64, y: L64, w: u32, signed: bool) -> Logic {
    ord_to_logic(cmp_l64(x, y, w, signed), false, true, true)
}

// ===========================================================================
// 層 1: 2 値
// ===========================================================================

/// 2 値の `x == y` (§11.4.5)。2 値では `==`, `===`, `==?`, casez/casex の照合はどれも同じ。
#[inline]
pub const fn eq2_l64(x: u64, y: u64, w: u32) -> bool {
    let _ = w;
    x == y
}

/// 2 値の `x != y` (§11.4.5)。
#[inline]
pub const fn ne2_l64(x: u64, y: u64, w: u32) -> bool {
    !eq2_l64(x, y, w)
}

/// 2 値の大小比較 (幅 `w`、1〜64)。
#[inline]
pub const fn cmp2_l64(x: u64, y: u64, w: u32, signed: bool) -> Ordering {
    let (x, y) = if signed {
        (
            (sign_extend(x, w) as i64 ^ i64::MIN) as u64,
            (sign_extend(y, w) as i64 ^ i64::MIN) as u64,
        )
    } else {
        (x, y)
    };
    if x < y {
        Ordering::Less
    } else if x == y {
        Ordering::Equal
    } else {
        Ordering::Greater
    }
}

/// 2 値の `x < y` (§11.4.4)。
#[inline]
pub const fn lt2_l64(x: u64, y: u64, w: u32, signed: bool) -> bool {
    matches!(cmp2_l64(x, y, w, signed), Ordering::Less)
}

/// 2 値の `x <= y` (§11.4.4)。
#[inline]
pub const fn le2_l64(x: u64, y: u64, w: u32, signed: bool) -> bool {
    !matches!(cmp2_l64(x, y, w, signed), Ordering::Greater)
}

/// 2 値の `x > y` (§11.4.4)。
#[inline]
pub const fn gt2_l64(x: u64, y: u64, w: u32, signed: bool) -> bool {
    matches!(cmp2_l64(x, y, w, signed), Ordering::Greater)
}

/// 2 値の `x >= y` (§11.4.4)。
#[inline]
pub const fn ge2_l64(x: u64, y: u64, w: u32, signed: bool) -> bool {
    !matches!(cmp2_l64(x, y, w, signed), Ordering::Less)
}

// ===========================================================================
// 層 2: 4 値
// ===========================================================================

/// ワードの 4 つ組 `(ax, bx, ay, by)` の反復子。
#[inline(always)]
fn quads<'a>(x: LogicRef<'a>, y: LogicRef<'a>) -> impl Iterator<Item = (u64, u64, u64, u64)> + 'a {
    x.aval()
        .iter()
        .zip(x.bval())
        .zip(y.aval().iter().zip(y.bval()))
        .map(|((&ax, &bx), (&ay, &by))| (ax, bx, ay, by))
}

/// `x == y` (§11.4.5) [要VCS確認 V-15]。
///
/// # Panics
/// 幅が違う場合。
pub fn eq(x: LogicRef<'_>, y: LogicRef<'_>) -> Logic {
    check_same_width("eq", x.width(), y.width());
    let mut unknown = false;
    for (ax, bx, ay, by) in quads(x, y) {
        let unk = bx | by;
        if (ax ^ ay) & !unk != 0 {
            return Logic::L0;
        }
        unknown |= unk != 0;
    }
    eq_result(false, unknown)
}

/// `x != y` (§11.4.5)。
///
/// # Panics
/// 幅が違う場合。
pub fn ne(x: LogicRef<'_>, y: LogicRef<'_>) -> Logic {
    check_same_width("ne", x.width(), y.width());
    eq(x, y).not()
}

/// `x === y` (§11.4.5)。x は x とだけ、z は z とだけ一致する。`case` 文の照合にも使う (§12.5)。
///
/// # Panics
/// 幅が違う場合。
pub fn case_eq(x: LogicRef<'_>, y: LogicRef<'_>) -> bool {
    check_same_width("case_eq", x.width(), y.width());
    x.aval() == y.aval() && x.bval() == y.bval()
}

/// `x !== y` (§11.4.5)。
///
/// # Panics
/// 幅が違う場合。
pub fn case_ne(x: LogicRef<'_>, y: LogicRef<'_>) -> bool {
    check_same_width("case_ne", x.width(), y.width());
    !case_eq(x, y)
}

/// `x ==? y` (§11.4.6) [要VCS確認 V-16]。右オペランドの x/z ビットはワイルドカードで比較しない。
///
/// # Panics
/// 幅が違う場合。
pub fn wildcard_eq(x: LogicRef<'_>, y: LogicRef<'_>) -> Logic {
    check_same_width("wildcard_eq", x.width(), y.width());
    let mut unknown = false;
    for (ax, bx, ay, by) in quads(x, y) {
        let care = !by;
        if (ax ^ ay) & care & !bx != 0 {
            return Logic::L0;
        }
        unknown |= bx & care != 0;
    }
    eq_result(false, unknown)
}

/// `x !=? y` (§11.4.6)。
///
/// # Panics
/// 幅が違う場合。
pub fn wildcard_ne(x: LogicRef<'_>, y: LogicRef<'_>) -> Logic {
    check_same_width("wildcard_ne", x.width(), y.width());
    wildcard_eq(x, y).not()
}

/// `casez` の照合 (§12.5.1) [要VCS確認 V-17]。どちらかが z (`?` を含む) のビットは無視し、
/// 残りを `===` で比べる。
///
/// # Panics
/// 幅が違う場合。
pub fn casez_match(x: LogicRef<'_>, y: LogicRef<'_>) -> bool {
    check_same_width("casez_match", x.width(), y.width());
    quads(x, y).all(|(ax, bx, ay, by)| {
        let ignore = (bx & !ax) | (by & !ay);
        ((ax ^ ay) | (bx ^ by)) & !ignore == 0
    })
}

/// `casex` の照合 (§12.5.1) [要VCS確認 V-17]。どちらかが x/z のビットは無視する。
///
/// # Panics
/// 幅が違う場合。
pub fn casex_match(x: LogicRef<'_>, y: LogicRef<'_>) -> bool {
    check_same_width("casex_match", x.width(), y.width());
    quads(x, y).all(|(ax, bx, ay, by)| (ax ^ ay) & !(bx | by) == 0)
}

fn relational(
    op: &str,
    x: LogicRef<'_>,
    y: LogicRef<'_>,
    signed: bool,
    lt: bool,
    eq: bool,
    gt: bool,
) -> Logic {
    check_same_width(op, x.width(), y.width());
    if x.has_unknown() || y.has_unknown() {
        return Logic::X;
    }
    ord_to_logic(
        Some(cmp_words(x.aval(), y.aval(), x.width(), signed)),
        lt,
        eq,
        gt,
    )
}

/// `x < y` (§11.4.4)。どちらかに未知ビットがあれば x。`signed` は両オペランドが符号付きのときだけ真
/// にする (§11.8.1、呼び出し側の責務)。
///
/// # Panics
/// 幅が違う場合。
pub fn lt(x: LogicRef<'_>, y: LogicRef<'_>, signed: bool) -> Logic {
    relational("lt", x, y, signed, true, false, false)
}

/// `x <= y` (§11.4.4)。
///
/// # Panics
/// 幅が違う場合。
pub fn le(x: LogicRef<'_>, y: LogicRef<'_>, signed: bool) -> Logic {
    relational("le", x, y, signed, true, true, false)
}

/// `x > y` (§11.4.4)。
///
/// # Panics
/// 幅が違う場合。
pub fn gt(x: LogicRef<'_>, y: LogicRef<'_>, signed: bool) -> Logic {
    relational("gt", x, y, signed, false, false, true)
}

/// `x >= y` (§11.4.4)。
///
/// # Panics
/// 幅が違う場合。
pub fn ge(x: LogicRef<'_>, y: LogicRef<'_>, signed: bool) -> Logic {
    relational("ge", x, y, signed, false, true, true)
}

// ===========================================================================
// 層 2: 2 値
// ===========================================================================

macro_rules! eq2_family {
    ($($name:ident => $doc:literal, $neg:literal;)*) => {$(
        #[doc = $doc]
        ///
        /// 2 値では x/z が無いので、`==`, `===`, `==?`, casez/casex の照合はどれもワード列の一致になる。
        ///
        /// # Panics
        /// 幅が違う場合。
        pub fn $name(x: BitRef<'_>, y: BitRef<'_>) -> bool {
            check_same_width(stringify!($name), x.width(), y.width());
            (x.words() == y.words()) != $neg
        }
    )*};
}

eq2_family! {
    eq2 => "2 値の `x == y` (§11.4.5)。", false;
    ne2 => "2 値の `x != y` (§11.4.5)。", true;
    case_eq2 => "2 値の `x === y` (§11.4.5)。", false;
    case_ne2 => "2 値の `x !== y` (§11.4.5)。", true;
    wildcard_eq2 => "2 値の `x ==? y` (§11.4.6)。", false;
    wildcard_ne2 => "2 値の `x !=? y` (§11.4.6)。", true;
    casez_match2 => "2 値の casez の照合 (§12.5.1)。", false;
    casex_match2 => "2 値の casex の照合 (§12.5.1)。", false;
}

macro_rules! rel2 {
    ($($name:ident => $doc:literal, $pred:expr;)*) => {$(
        #[doc = $doc]
        ///
        /// # Panics
        /// 幅が違う場合。
        pub fn $name(x: BitRef<'_>, y: BitRef<'_>, signed: bool) -> bool {
            check_same_width(stringify!($name), x.width(), y.width());
            let pred: fn(Ordering) -> bool = $pred;
            pred(cmp_words(x.words(), y.words(), x.width(), signed))
        }
    )*};
}

rel2! {
    lt2 => "2 値の `x < y` (§11.4.4)。", Ordering::is_lt;
    le2 => "2 値の `x <= y` (§11.4.4)。", Ordering::is_le;
    gt2 => "2 値の `x > y` (§11.4.4)。", Ordering::is_gt;
    ge2 => "2 値の `x >= y` (§11.4.4)。", Ordering::is_ge;
}

// ===========================================================================
// 層 3
// ===========================================================================

/// 注意: [`LogicVec::eq`] / [`BitVec::eq`] は `PartialEq::eq` をメソッド呼び出しで隠す
/// (`==` 演算子は `PartialEq` のまま、つまり `===` の意味)。
impl LogicVec {
    /// `self == y` (§11.4.5)。`PartialEq` (`==` 演算子、`===` の意味) とは別物。
    #[allow(clippy::should_implement_trait)] // 層 3 の名前は層 2 のカーネルと揃える (plan.md §3.3)
    pub fn eq(&self, y: &LogicVec) -> Logic {
        eq(self.as_ref(), y.as_ref())
    }
    /// `self != y` (§11.4.5)。
    #[allow(clippy::should_implement_trait)]
    pub fn ne(&self, y: &LogicVec) -> Logic {
        ne(self.as_ref(), y.as_ref())
    }
    /// `self === y` (§11.4.5)。幅が違えば panic する点を除き `PartialEq` と同じ。
    pub fn case_eq(&self, y: &LogicVec) -> bool {
        case_eq(self.as_ref(), y.as_ref())
    }
    /// `self !== y` (§11.4.5)。
    pub fn case_ne(&self, y: &LogicVec) -> bool {
        case_ne(self.as_ref(), y.as_ref())
    }
    /// `self ==? y` (§11.4.6)。
    pub fn wildcard_eq(&self, y: &LogicVec) -> Logic {
        wildcard_eq(self.as_ref(), y.as_ref())
    }
    /// `self !=? y` (§11.4.6)。
    pub fn wildcard_ne(&self, y: &LogicVec) -> Logic {
        wildcard_ne(self.as_ref(), y.as_ref())
    }
    /// casez の照合 (§12.5.1)。
    pub fn casez_match(&self, y: &LogicVec) -> bool {
        casez_match(self.as_ref(), y.as_ref())
    }
    /// casex の照合 (§12.5.1)。
    pub fn casex_match(&self, y: &LogicVec) -> bool {
        casex_match(self.as_ref(), y.as_ref())
    }
    /// `self < y` (§11.4.4)。
    pub fn lt(&self, y: &LogicVec, signed: bool) -> Logic {
        lt(self.as_ref(), y.as_ref(), signed)
    }
    /// `self <= y` (§11.4.4)。
    pub fn le(&self, y: &LogicVec, signed: bool) -> Logic {
        le(self.as_ref(), y.as_ref(), signed)
    }
    /// `self > y` (§11.4.4)。
    pub fn gt(&self, y: &LogicVec, signed: bool) -> Logic {
        gt(self.as_ref(), y.as_ref(), signed)
    }
    /// `self >= y` (§11.4.4)。
    pub fn ge(&self, y: &LogicVec, signed: bool) -> Logic {
        ge(self.as_ref(), y.as_ref(), signed)
    }
}

impl BitVec {
    /// 2 値の `self == y` (§11.4.5)。幅が違えば panic する点を除き `PartialEq` と同じ。
    #[allow(clippy::should_implement_trait)] // 層 3 の名前は層 2 のカーネルと揃える (plan.md §3.3)
    pub fn eq(&self, y: &BitVec) -> bool {
        eq2(self.as_ref(), y.as_ref())
    }
    /// 2 値の `self != y` (§11.4.5)。
    #[allow(clippy::should_implement_trait)]
    pub fn ne(&self, y: &BitVec) -> bool {
        ne2(self.as_ref(), y.as_ref())
    }
    /// 2 値の `self === y` (§11.4.5)。
    pub fn case_eq(&self, y: &BitVec) -> bool {
        case_eq2(self.as_ref(), y.as_ref())
    }
    /// 2 値の `self !== y` (§11.4.5)。
    pub fn case_ne(&self, y: &BitVec) -> bool {
        case_ne2(self.as_ref(), y.as_ref())
    }
    /// 2 値の `self ==? y` (§11.4.6)。
    pub fn wildcard_eq(&self, y: &BitVec) -> bool {
        wildcard_eq2(self.as_ref(), y.as_ref())
    }
    /// 2 値の `self !=? y` (§11.4.6)。
    pub fn wildcard_ne(&self, y: &BitVec) -> bool {
        wildcard_ne2(self.as_ref(), y.as_ref())
    }
    /// 2 値の casez の照合 (§12.5.1)。
    pub fn casez_match(&self, y: &BitVec) -> bool {
        casez_match2(self.as_ref(), y.as_ref())
    }
    /// 2 値の casex の照合 (§12.5.1)。
    pub fn casex_match(&self, y: &BitVec) -> bool {
        casex_match2(self.as_ref(), y.as_ref())
    }
    /// 2 値の `self < y` (§11.4.4)。
    pub fn lt(&self, y: &BitVec, signed: bool) -> bool {
        lt2(self.as_ref(), y.as_ref(), signed)
    }
    /// 2 値の `self <= y` (§11.4.4)。
    pub fn le(&self, y: &BitVec, signed: bool) -> bool {
        le2(self.as_ref(), y.as_ref(), signed)
    }
    /// 2 値の `self > y` (§11.4.4)。
    pub fn gt(&self, y: &BitVec, signed: bool) -> bool {
        gt2(self.as_ref(), y.as_ref(), signed)
    }
    /// 2 値の `self >= y` (§11.4.4)。
    pub fn ge(&self, y: &BitVec, signed: bool) -> bool {
        ge2(self.as_ref(), y.as_ref(), signed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Logic::{L0 as O, L1 as I, X};

    fn v(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    /// §11.4.5 の例 [要VCS確認 V-15]。
    #[test]
    fn equality_11_4_5() {
        assert_eq!(v("1010").eq(&v("1010")), I);
        assert_eq!(v("1x00").eq(&v("0x00")), O);
        assert_eq!(v("1x00").eq(&v("1000")), X);
        assert_eq!(v("1z00").ne(&v("1z00")), X);
        assert!(v("1z00").case_eq(&v("1z00")));
        assert!(!v("1z00").case_eq(&v("1x00")));
    }

    /// §11.4.6 [要VCS確認 V-16]。
    #[test]
    fn wildcard_11_4_6() {
        assert_eq!(v("1010").wildcard_eq(&v("1x1z")), I);
        assert_eq!(v("1x10").wildcard_eq(&v("1x1z")), I);
        assert_eq!(v("1x10").wildcard_eq(&v("1010")), X);
        assert_eq!(v("0x10").wildcard_eq(&v("1010")), O);
        assert_eq!(v("1x1z").wildcard_eq(&v("1010")), X);
    }

    /// §12.5.1 [要VCS確認 V-17]。
    #[test]
    fn casez_casex_12_5_1() {
        assert!(v("1z01").casez_match(&v("1101")));
        assert!(!v("1x01").casez_match(&v("1101")));
        assert!(v("1x01").casex_match(&v("1101")));
        assert!(!v("0x01").casex_match(&v("1101")));
        assert!(!v("0000").casez_match(&v("xxxx")));
        assert!(v("0000").casex_match(&v("xxxx")));
    }

    /// §11.4.4 と §11.8.1 (符号付き比較)。
    #[test]
    fn relational_11_4_4() {
        let a = LogicVec::from_u64(8, 0x80);
        let b = LogicVec::from_u64(8, 0x7f);
        assert_eq!(a.lt(&b, true), I);
        assert_eq!(a.lt(&b, false), O);
        assert_eq!(v("0001").lt(&v("001x"), false), X);
        let mut big = LogicVec::zero(65);
        big.set_bit(64, I);
        assert_eq!(big.lt(&LogicVec::zero(65), true), I);
        assert_eq!(big.gt(&LogicVec::from_u64(65, u64::MAX), false), I);
        assert_eq!(lt_l64(L64::new(0x80, 0), L64::new(0x7f, 0), 8, true), I);
        assert!(lt2_l64(0xff, 0, 8, true));
        assert!(BitVec::from_u64(8, 0xff).lt(&BitVec::zero(8), true));
    }
}
