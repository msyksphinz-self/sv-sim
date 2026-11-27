//! 参照モデルの値: 1 ビットの 4 値 `RefLogic` と、ビット列 `RefVec` (plan.md §5.1)。
//!
//! M0-T1 のスタブ。公開 API の形だけを固定し、本体は M0-T9 で実装する。

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

/// 参照モデルのビット列。要素 0 が LSB で、1 要素が 1 ビット。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RefVec(pub Vec<RefLogic>);

impl RefVec {
    /// bits 文字列 (MSB が先頭、`'_'` は無視、`0 1 x z`) から作る。
    /// 形式は `svsim-logic` の `LogicVec::from_bits_str` と同じ。
    pub fn from_bits_str(s: &str) -> Result<RefVec, String> {
        unimplemented!("RefVec::from_bits_str({s:?}) is implemented in M0-T9")
    }

    /// bits 文字列 (MSB が先頭、`'_'` なし、小文字の `x`/`z`) に変換する。
    /// `from_bits_str` と往復する。
    pub fn to_bits_string(&self) -> String {
        unimplemented!("RefVec::to_bits_string is implemented in M0-T9")
    }

    /// ビット幅。
    pub fn width(&self) -> usize {
        unimplemented!("RefVec::width is implemented in M0-T9")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // スタブ段階で確認できるのは型の形だけ。値の意味論のテストは M0-T9 で書く。
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
    }
}
