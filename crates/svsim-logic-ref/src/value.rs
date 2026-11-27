//! 参照モデルの値: 1 ビットの 4 値 `RefLogic` と、ビット列 `RefVec` (plan.md §5.1)。
//!
//! 整数との相互変換 (`BigUint` / `BigInt`) もここに置く。算術・比較・実数変換などは
//! 「X/Z が無ければ `BigInt` に変換して計算し、`2^w` の剰余を取って戻す」方針 (§5.1) なので、
//! その変換を 1 か所にまとめる。

use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Zero};

/// 1 ビットの 4 値 (IEEE 1800-2023 §6.3.1 の 0, 1, x, z)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefLogic {
    /// 論理 0。
    Zero,
    /// 論理 1。
    One,
    /// 不定値 x。
    X,
    /// ハイインピーダンス z。
    Z,
}

impl RefLogic {
    /// 全 4 値 (網羅テスト用)。順序は 0, 1, x, z。
    pub const ALL: [RefLogic; 4] = [RefLogic::Zero, RefLogic::One, RefLogic::X, RefLogic::Z];

    /// 真理値表の添字 (0, 1, x, z の順で 0..4)。
    pub(crate) fn idx(self) -> usize {
        match self {
            RefLogic::Zero => 0,
            RefLogic::One => 1,
            RefLogic::X => 2,
            RefLogic::Z => 3,
        }
    }

    /// X または Z なら真。
    pub fn is_unknown(self) -> bool {
        matches!(self, RefLogic::X | RefLogic::Z)
    }

    /// `bool` から作る (true = 1)。
    pub fn from_bool(b: bool) -> RefLogic {
        if b { RefLogic::One } else { RefLogic::Zero }
    }

    /// bits 文字列の 1 文字 (小文字)。
    pub fn to_char(self) -> char {
        match self {
            RefLogic::Zero => '0',
            RefLogic::One => '1',
            RefLogic::X => 'x',
            RefLogic::Z => 'z',
        }
    }

    /// bits 文字列の 1 文字から作る。大文字の `X`/`Z` も受ける。
    pub fn from_char(c: char) -> Option<RefLogic> {
        match c {
            '0' => Some(RefLogic::Zero),
            '1' => Some(RefLogic::One),
            'x' | 'X' => Some(RefLogic::X),
            'z' | 'Z' => Some(RefLogic::Z),
            _ => None,
        }
    }

    /// 1 ビットの AND (§11.4.8 の表。`&&` も同じ表、§11.4.7)。
    pub fn and(self, other: RefLogic) -> RefLogic {
        crate::bitwise::AND_TABLE[self.idx()][other.idx()]
    }

    /// 1 ビットの OR (§11.4.8 の表。`||` も同じ表、§11.4.7)。
    pub fn or(self, other: RefLogic) -> RefLogic {
        crate::bitwise::OR_TABLE[self.idx()][other.idx()]
    }

    /// 論理含意 `a -> b` = `!a || b` (§11.4.7)。
    pub fn implies(self, other: RefLogic) -> RefLogic {
        (!self).or(other)
    }

    /// 論理同値 `a <-> b` = `(a -> b) && (b -> a)` (§11.4.7)。
    pub fn equiv(self, other: RefLogic) -> RefLogic {
        self.implies(other).and(other.implies(self))
    }
}

/// 1 ビットの `!` / `~` (§11.4.7, §11.4.8 の NOT の表)。
impl std::ops::Not for RefLogic {
    type Output = RefLogic;
    fn not(self) -> RefLogic {
        crate::bitwise::NOT_TABLE[self.idx()]
    }
}

/// 参照モデルのビット列。要素 0 が LSB で、1 要素が 1 ビット。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RefVec(pub Vec<RefLogic>);

impl RefVec {
    /// bits 文字列 (MSB が先頭、`0 1 x X z Z`) から作る。
    /// 形式は `svsim-logic` の `LogicVec::from_bits_str` と同じ (マネージャ決定 Q-22)。
    ///
    /// 大文字と小文字は同一視する。`'_'` を含むそれ以外の文字はエラー。
    /// 空文字列は幅 0 の値になる。
    pub fn from_bits_str(s: &str) -> Result<RefVec, String> {
        let mut bits = Vec::with_capacity(s.len());
        for (pos, c) in s.chars().enumerate() {
            match RefLogic::from_char(c) {
                Some(b) => bits.push(b),
                None => return Err(format!("invalid bit character {c:?} at position {pos}")),
            }
        }
        // 文字列は MSB が先頭なので、LSB 先頭の並びに反転する。
        bits.reverse();
        Ok(RefVec(bits))
    }

    /// bits 文字列 (MSB が先頭、`'_'` なし、小文字の `x`/`z`) に変換する。
    /// `from_bits_str` と往復する。
    pub fn to_bits_string(&self) -> String {
        self.0.iter().rev().map(|b| b.to_char()).collect()
    }

    /// ビット幅。
    pub fn width(&self) -> usize {
        self.0.len()
    }

    /// 全ビットが `v` の幅 `width` の値。
    pub fn filled(width: usize, v: RefLogic) -> RefVec {
        RefVec(vec![v; width])
    }

    /// 全ビットが 0 の値。
    pub fn zeros(width: usize) -> RefVec {
        RefVec::filled(width, RefLogic::Zero)
    }

    /// 全ビットが X の値。
    pub fn all_x(width: usize) -> RefVec {
        RefVec::filled(width, RefLogic::X)
    }

    /// `u64` の下位 `width` ビットから作る (幅が 64 より広ければ上位は 0)。
    pub fn from_u64(width: usize, v: u64) -> RefVec {
        RefVec(
            (0..width)
                .map(|i| RefLogic::from_bool(i < 64 && (v >> i) & 1 == 1))
                .collect(),
        )
    }

    /// `i64` を 2 の補数で幅 `width` に切り詰める/符号拡張して作る。
    pub fn from_i64(width: usize, v: i64) -> RefVec {
        RefVec::from_bigint(width, &BigInt::from(v))
    }

    /// ビット `i` (LSB = 0)。
    pub fn bit(&self, i: usize) -> RefLogic {
        self.0[i]
    }

    /// 最上位ビット (幅 0 なら `None`)。
    pub fn msb(&self) -> Option<RefLogic> {
        self.0.last().copied()
    }

    /// X または Z のビットがあれば真。
    pub fn has_unknown(&self) -> bool {
        self.0.iter().any(|b| b.is_unknown())
    }

    /// 符号なし整数として読む。X/Z があれば `None`。
    pub fn to_biguint(&self) -> Option<BigUint> {
        if self.has_unknown() {
            return None;
        }
        Some(self.to_biguint_xz_as_zero())
    }

    /// 符号なし整数として読む。X/Z は 0 として扱う。
    pub fn to_biguint_xz_as_zero(&self) -> BigUint {
        let mut v = BigUint::zero();
        for (i, b) in self.0.iter().enumerate() {
            if *b == RefLogic::One {
                v.set_bit(i as u64, true);
            }
        }
        v
    }

    /// 整数として読む。`signed` なら 2 の補数 (MSB が符号)。X/Z があれば `None`。
    pub fn to_bigint(&self, signed: bool) -> Option<BigInt> {
        if self.has_unknown() {
            return None;
        }
        Some(self.to_bigint_xz_as_zero(signed))
    }

    /// 整数として読む。X/Z は 0 として扱う。
    pub fn to_bigint_xz_as_zero(&self, signed: bool) -> BigInt {
        let u = BigInt::from_biguint(Sign::Plus, self.to_biguint_xz_as_zero());
        if signed && self.msb() == Some(RefLogic::One) {
            u - (BigInt::one() << self.width())
        } else {
            u
        }
    }

    /// 符号なし整数の下位 `width` ビットから作る。
    pub fn from_biguint(width: usize, v: &BigUint) -> RefVec {
        RefVec(
            (0..width)
                .map(|i| RefLogic::from_bool(v.bit(i as u64)))
                .collect(),
        )
    }

    /// 整数を `2^width` の剰余 (2 の補数) で幅 `width` にして作る。
    pub fn from_bigint(width: usize, v: &BigInt) -> RefVec {
        let modulus = BigInt::one() << width;
        let mut r = v % &modulus;
        if r.sign() == Sign::Minus {
            r += &modulus;
        }
        let u = r.to_biguint().expect("non-negative after adjustment");
        RefVec::from_biguint(width, &u)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ref_vec_is_lsb_first_vec_of_ref_logic() {
        let v = RefVec(vec![
            RefLogic::One,
            RefLogic::Zero,
            RefLogic::X,
            RefLogic::Z,
        ]);
        assert_eq!(v.0[0], RefLogic::One);
        assert_eq!(v.clone(), v);
        assert_eq!(v.to_bits_string(), "zx01");
    }

    #[test]
    fn bits_str_round_trip_and_rejects_underscore() {
        let v = RefVec::from_bits_str("10xz0001").unwrap();
        assert_eq!(v.width(), 8);
        assert_eq!(v.bit(0), RefLogic::One);
        assert_eq!(v.bit(4), RefLogic::Z);
        assert_eq!(v.bit(7), RefLogic::One);
        assert_eq!(v.to_bits_string(), "10xz0001");
        assert_eq!(RefVec::from_bits_str("XZ").unwrap().to_bits_string(), "xz");
        assert_eq!(RefVec::from_bits_str("").unwrap().width(), 0);
        assert!(RefVec::from_bits_str("102").is_err());
        assert!(RefVec::from_bits_str("1?").is_err());
        // Q-22: '_' はエラー
        assert!(RefVec::from_bits_str("10_01").is_err());
        assert!(RefVec::from_bits_str("_").is_err());
    }

    #[test]
    fn integer_conversions() {
        let v = RefVec::from_bits_str("11111111").unwrap();
        assert_eq!(v.to_bigint(true), Some(BigInt::from(-1)));
        assert_eq!(v.to_bigint(false), Some(BigInt::from(255)));
        assert_eq!(RefVec::from_i64(8, -1), v);
        assert_eq!(RefVec::from_i64(4, -8).to_bits_string(), "1000");
        assert_eq!(RefVec::from_u64(70, 5).to_bits_string().len(), 70);
        assert_eq!(RefVec::from_bits_str("1x").unwrap().to_biguint(), None);
        assert_eq!(
            RefVec::from_bits_str("1x").unwrap().to_biguint_xz_as_zero(),
            BigUint::from(2u32)
        );
    }
}
