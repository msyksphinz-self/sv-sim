//! 1 ビットの 4 値 [`Logic`] (IEEE 1800-2023 §6.3.1、plan.md §3.1)。

use std::fmt;

/// 1 ビットの 4 値。
///
/// 値は `aval | (bval << 1)` で、`svdpi.h` (Annex I) の `sv_0 = 0, sv_1 = 1, sv_z = 2, sv_x = 3`
/// と一致する。そのため DPI のスカラ `svLogic` とは変換なしで受け渡せる。
///
/// `Display` は `'0' '1' 'z' 'x'` (小文字) を出す。
#[repr(u8)]
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Logic {
    /// 論理 0 (aval = 0, bval = 0)。
    L0 = 0,
    /// 論理 1 (aval = 1, bval = 0)。
    L1 = 1,
    /// ハイインピーダンス z (aval = 0, bval = 1)。
    Z = 2,
    /// 不定値 x (aval = 1, bval = 1)。
    X = 3,
}

impl Logic {
    /// 4 つの値すべて (repr の順)。網羅テスト用。
    pub const ALL: [Logic; 4] = [Logic::L0, Logic::L1, Logic::Z, Logic::X];

    /// aval と bval のビットから作る。
    #[inline]
    pub const fn from_ab(a: bool, b: bool) -> Logic {
        match (a, b) {
            (false, false) => Logic::L0,
            (true, false) => Logic::L1,
            (false, true) => Logic::Z,
            (true, true) => Logic::X,
        }
    }

    /// aval/bval のワードの最下位ビットから作る (上位ビットは無視する)。
    #[inline]
    pub const fn from_words_lsb(a: u64, b: u64) -> Logic {
        Logic::from_ab(a & 1 != 0, b & 1 != 0)
    }

    /// `svLogic` の値 (0..=3) から作る。範囲外なら `None`。
    #[inline]
    pub const fn from_u8(v: u8) -> Option<Logic> {
        match v {
            0 => Some(Logic::L0),
            1 => Some(Logic::L1),
            2 => Some(Logic::Z),
            3 => Some(Logic::X),
            _ => None,
        }
    }

    /// `svLogic` の値 (0..=3)。
    #[inline]
    pub const fn to_u8(self) -> u8 {
        self as u8
    }

    /// aval のビット。
    #[inline]
    pub const fn aval(self) -> bool {
        (self as u8) & 1 != 0
    }

    /// bval のビット。
    #[inline]
    pub const fn bval(self) -> bool {
        (self as u8) & 2 != 0
    }

    /// X または Z なら真 (bval だけで判定できる)。
    #[inline]
    pub const fn is_unknown(self) -> bool {
        self.bval()
    }

    /// 0 または 1 なら真。
    #[inline]
    pub const fn is_known(self) -> bool {
        !self.bval()
    }

    /// 既知の値なら `Some(bool)`、X/Z なら `None`。
    #[inline]
    pub const fn to_bool(self) -> Option<bool> {
        match self {
            Logic::L0 => Some(false),
            Logic::L1 => Some(true),
            Logic::Z | Logic::X => None,
        }
    }

    /// 2 値の型に格納したときの値 (X/Z → 0、§6.11.2)。
    #[inline]
    pub const fn to_two_state(self) -> bool {
        matches!(self, Logic::L1)
    }

    /// bits 文字列での 1 文字 (`'0' '1' 'z' 'x'`)。
    #[inline]
    pub const fn to_char(self) -> char {
        match self {
            Logic::L0 => '0',
            Logic::L1 => '1',
            Logic::Z => 'z',
            Logic::X => 'x',
        }
    }

    /// bits 文字列の 1 文字から作る。`0 1 x X z Z` を受け付け、それ以外は `None`。
    #[inline]
    pub const fn from_char(c: char) -> Option<Logic> {
        match c {
            '0' => Some(Logic::L0),
            '1' => Some(Logic::L1),
            'z' | 'Z' => Some(Logic::Z),
            'x' | 'X' => Some(Logic::X),
            _ => None,
        }
    }
}

impl From<bool> for Logic {
    #[inline]
    fn from(v: bool) -> Logic {
        if v { Logic::L1 } else { Logic::L0 }
    }
}

impl fmt::Display for Logic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use fmt::Write;
        f.write_char(self.to_char())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Annex I (`svdpi.h`) の sv_0/sv_1/sv_z/sv_x と repr が一致する (plan.md §3.1)。
    #[test]
    fn repr_matches_svdpi() {
        assert_eq!(Logic::L0 as u8, 0);
        assert_eq!(Logic::L1 as u8, 1);
        assert_eq!(Logic::Z as u8, 2);
        assert_eq!(Logic::X as u8, 3);
        assert_eq!(std::mem::size_of::<Logic>(), 1);
    }

    /// aval/bval の割り当ては s_vpi_vecval / svLogicVecVal と同じ (plan.md §3.1)。
    #[test]
    fn aval_bval_encoding() {
        let table = [
            (Logic::L0, false, false),
            (Logic::L1, true, false),
            (Logic::Z, false, true),
            (Logic::X, true, true),
        ];
        for (v, a, b) in table {
            assert_eq!(v.aval(), a, "{v:?}");
            assert_eq!(v.bval(), b, "{v:?}");
            assert_eq!(Logic::from_ab(a, b), v);
            assert_eq!(Logic::from_words_lsb(a as u64 | 2, b as u64 | 4), v);
            assert_eq!(v.is_unknown(), b);
            assert_eq!(v.is_known(), !b);
            assert_eq!(v.to_u8(), (a as u8) | ((b as u8) << 1));
        }
    }

    #[test]
    fn u8_round_trip() {
        for v in Logic::ALL {
            assert_eq!(Logic::from_u8(v.to_u8()), Some(v));
        }
        assert_eq!(Logic::from_u8(4), None);
        assert_eq!(Logic::from_u8(255), None);
    }

    #[test]
    fn char_round_trip_and_display() {
        for v in Logic::ALL {
            assert_eq!(Logic::from_char(v.to_char()), Some(v));
            assert_eq!(v.to_string(), v.to_char().to_string());
        }
        assert_eq!(Logic::from_char('X'), Some(Logic::X));
        assert_eq!(Logic::from_char('Z'), Some(Logic::Z));
        assert_eq!(Logic::from_char('?'), None);
        assert_eq!(Logic::from_char('2'), None);
        assert_eq!(Logic::Z.to_string(), "z");
        assert_eq!(Logic::X.to_string(), "x");
    }

    /// 2 値型への格納では X/Z が 0 になる (§6.11.2)。
    #[test]
    fn to_two_state_maps_unknown_to_zero() {
        assert!(!Logic::L0.to_two_state());
        assert!(Logic::L1.to_two_state());
        assert!(!Logic::Z.to_two_state());
        assert!(!Logic::X.to_two_state());
        assert_eq!(Logic::X.to_bool(), None);
        assert_eq!(Logic::L1.to_bool(), Some(true));
        assert_eq!(Logic::from(true), Logic::L1);
        assert_eq!(Logic::from(false), Logic::L0);
    }
}
