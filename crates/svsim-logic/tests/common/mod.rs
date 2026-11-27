//! `svsim-logic` の統合テストの共通部品 (plan.md §5.2, §5.3)。
//!
//! - proptest の入力の戦略 (幅、値、X/Z の密度、シフト量、選択のオフセット、除数、指数)。
//! - 参照モデル `svsim-logic-ref` との値の受け渡し (bits 文字列経由、plan.md §3.7)。
//! - 出力の検査 (正規化不変条件 plan.md §3.2、参照モデルとの全ビット一致)。
//! - 層 2 のカーネルをプレーンが隣接しないビューで試すための [`SplitLogic`] / [`SplitBits`]。
//!
//! 使い方: 各テストファイルの先頭で `mod common;` と書く。T4〜T8 はこのファイルを**読むだけ**にし、
//! 追加の戦略は各テストファイルの中に書いた (plan.md §3.7)。M0-T14 で、複数のテストファイルに重複
//! していた変換・検査の補助 (`bool_to_ref`、`edge_to_ref`、`all_logic`、`check_unchanged` など) を
//! ここに寄せた。
//!
//! 参照モデルの API は `RefVec::{from_bits_str, to_bits_string, width}` だけを使う (plan.md §5.1)。
//!
//! 件数は proptest の既定 (`PROPTEST_CASES` 環境変数、未設定なら 256) に従う。

#![allow(dead_code)]

use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::ops::edge::Edge;
use svsim_logic::{BitMut, BitRef, BitVec, L64, Logic, LogicMut, LogicRef, LogicVec, words_for};
use svsim_logic_ref::{RefEdge, RefLogic, RefVec};

// ===========================================================================
// 幅
// ===========================================================================

/// 境界の幅 (plan.md §5.3)。
pub const BOUNDARY_WIDTHS: [u32; 20] = [
    1, 2, 3, 7, 8, 31, 32, 33, 63, 64, 65, 127, 128, 129, 191, 192, 193, 255, 256, 257,
];

/// 層 1 (幅 ≤ 64) の境界の幅。
pub const SCALAR_BOUNDARY_WIDTHS: [u32; 10] = [1, 2, 3, 7, 8, 31, 32, 33, 63, 64];

/// 幅の戦略 (plan.md §5.3): 40% 境界値、40% `1..=200`、20% `201..=1024`。
pub fn width() -> impl Strategy<Value = u32> + Clone {
    prop_oneof![
        4 => prop::sample::select(&BOUNDARY_WIDTHS[..]),
        4 => 1u32..=200,
        2 => 201u32..=1024,
    ]
}

/// 層 1 (幅 ≤ 64) 用の幅の戦略: 50% 境界値、50% `1..=64`。
pub fn scalar_width() -> impl Strategy<Value = u32> + Clone {
    prop_oneof![prop::sample::select(&SCALAR_BOUNDARY_WIDTHS[..]), 1u32..=64,]
}

// ===========================================================================
// 小さな決定的 PRNG (X/Z の密度のビット列を作るため)
// ===========================================================================

/// splitmix64。proptest の乱数 (縮小できる `u64` のシード) から多数のビットを作るのに使う。
#[derive(Clone, Debug)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    /// 次の 64 ビット。
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 各ビットが確率 `1/den` で 1 になる 64 ビット。
    pub fn sparse_u64(&mut self, den: u64) -> u64 {
        let mut w = 0u64;
        for i in 0..64 {
            if self.next_u64().is_multiple_of(den) {
                w |= 1 << i;
            }
        }
        w
    }
}

// ===========================================================================
// 値 (aval プレーン)
// ===========================================================================

/// 1 ワードの模様 (plan.md §5.3「ワードごとに、全 0 / 全 1 / 乱数 / 1 ビットだけ 1 / 符号ビットだけ 1」)。
fn word_pattern() -> impl Strategy<Value = u64> {
    prop_oneof![
        1 => Just(0u64),
        1 => Just(u64::MAX),
        3 => any::<u64>(),
        1 => (0u32..64).prop_map(|i| 1u64 << i),
        1 => Just(1u64 << 63),
        1 => Just(i64::MAX as u64),
    ]
}

/// 値全体の模様 (幅 `w` で意味を持つもの)。
#[derive(Clone, Copy, Debug)]
enum WholeValue {
    Zero,
    Ones,
    /// 符号ビット (ビット `w - 1`) だけ 1 (符号付きの最小値)。
    SignOnly,
    /// 符号付きの最大値 (符号ビット以外が 1)。
    SignedMax,
    /// 1 だけ。
    One,
}

fn whole_value_words(w: u32, kind: WholeValue) -> Vec<u64> {
    let mut v = LogicVec::zero(w);
    match kind {
        WholeValue::Zero => {}
        WholeValue::Ones => v = LogicVec::filled(w, Logic::L1),
        WholeValue::SignOnly => v.set_bit(w - 1, Logic::L1),
        WholeValue::SignedMax => {
            v = LogicVec::filled(w, Logic::L1);
            v.set_bit(w - 1, Logic::L0);
        }
        WholeValue::One => v.set_bit(0, Logic::L1),
    }
    v.as_ref().aval().to_vec()
}

/// 幅 `w` の 2 値のワード列 (正規化済み)。ワードごとの模様と、値全体の模様を混ぜる。
pub fn known_words(w: u32) -> BoxedStrategy<Vec<u64>> {
    let nw = words_for(w);
    if nw == 0 {
        return Just(Vec::new()).boxed();
    }
    let per_word = prop::collection::vec(word_pattern(), nw).prop_map(move |mut v| {
        normalize(w, &mut v);
        v
    });
    let whole = prop::sample::select(vec![
        WholeValue::Zero,
        WholeValue::Ones,
        WholeValue::SignOnly,
        WholeValue::SignedMax,
        WholeValue::One,
    ])
    .prop_map(move |k| whole_value_words(w, k));
    prop_oneof![6 => per_word, 4 => whole].boxed()
}

fn normalize(w: u32, words: &mut [u64]) {
    if let Some(top) = words.last_mut() {
        *top &= svsim_logic::top_mask(w);
    }
}

// ===========================================================================
// X/Z
// ===========================================================================

/// X/Z のビットの密度 (plan.md §5.3: `{0, 1 ビットだけ, 1%, 50%, 全部}`)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownDensity {
    /// X/Z なし。
    None,
    /// 1 ビットだけ。
    Single,
    /// 各ビット 1%。
    Percent1,
    /// 各ビット 50%。
    Half,
    /// 全ビット。
    All,
}

/// 未知のビットのうち X にするものの比率。残りは Z。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum XzRatio {
    /// 全部 X。
    AllX,
    /// 全部 Z。
    AllZ,
    /// ビットごとに 50% で X か Z。
    Mixed,
}

/// 4 値の値の作り方 (X/Z の密度の重み)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// 一般の 4 値: X/Z なしは 3/10。
    General,
    /// 算術向け: X/Z なしを 8/12 にする (意味のある計算を多く試すため、plan.md §5.3)。
    Arith,
}

fn density(profile: Profile) -> impl Strategy<Value = UnknownDensity> {
    let none = match profile {
        Profile::General => 3,
        Profile::Arith => 8,
    };
    prop_oneof![
        none => Just(UnknownDensity::None),
        2 => Just(UnknownDensity::Single),
        2 => Just(UnknownDensity::Percent1),
        2 => Just(UnknownDensity::Half),
        1 => Just(UnknownDensity::All),
    ]
}

fn ratio() -> impl Strategy<Value = XzRatio> {
    prop::sample::select(vec![XzRatio::AllX, XzRatio::AllZ, XzRatio::Mixed])
}

/// aval のワード列に X/Z を入れて [`LogicVec`] にする。
pub fn inject_unknowns(
    w: u32,
    aval: &[u64],
    density: UnknownDensity,
    ratio: XzRatio,
    seed: u64,
) -> LogicVec {
    let nw = words_for(w);
    assert_eq!(aval.len(), nw);
    let mut rng = SplitMix64(seed);
    let mut unk = vec![0u64; nw];
    match density {
        UnknownDensity::None => {}
        UnknownDensity::Single => {
            if w > 0 {
                let i = (rng.next_u64() % w as u64) as usize;
                unk[i / 64] |= 1 << (i % 64);
            }
        }
        UnknownDensity::Percent1 => unk.iter_mut().for_each(|u| *u = rng.sparse_u64(100)),
        UnknownDensity::Half => unk.iter_mut().for_each(|u| *u = rng.next_u64()),
        UnknownDensity::All => unk.iter_mut().for_each(|u| *u = !0),
    }
    normalize(w, &mut unk);
    let mut v = LogicVec::zero(w);
    {
        let (a, b) = v.as_mut().into_planes();
        for i in 0..nw {
            let xsel = match ratio {
                XzRatio::AllX => !0,
                XzRatio::AllZ => 0,
                XzRatio::Mixed => rng.next_u64(),
            };
            // X = (a=1, b=1)、Z = (a=0, b=1) (plan.md §3.1)
            a[i] = (aval[i] & !unk[i]) | (unk[i] & xsel);
            b[i] = unk[i];
        }
    }
    v
}

/// 幅 `w` の 4 値の値 (一般の密度)。
pub fn logic_vec(w: u32) -> BoxedStrategy<LogicVec> {
    logic_vec_with(w, Profile::General)
}

/// 幅 `w` の 4 値の値 (算術向け: X/Z なしの比率が高い)。
pub fn logic_vec_arith(w: u32) -> BoxedStrategy<LogicVec> {
    logic_vec_with(w, Profile::Arith)
}

/// 幅 `w` の 4 値の値 (密度の重みを指定)。
pub fn logic_vec_with(w: u32, profile: Profile) -> BoxedStrategy<LogicVec> {
    (known_words(w), density(profile), ratio(), any::<u64>())
        .prop_map(move |(a, d, r, seed)| inject_unknowns(w, &a, d, r, seed))
        .boxed()
}

/// 幅 `w` の、X/Z を含まない 4 値の値。
pub fn logic_vec_known(w: u32) -> BoxedStrategy<LogicVec> {
    known_words(w)
        .prop_map(move |a| LogicVec::from(&bit_vec_from_words(w, &a)))
        .boxed()
}

/// 幅 `w` の 2 値の値。
pub fn bit_vec(w: u32) -> BoxedStrategy<BitVec> {
    known_words(w)
        .prop_map(move |a| bit_vec_from_words(w, &a))
        .boxed()
}

fn bit_vec_from_words(w: u32, a: &[u64]) -> BitVec {
    let mut v = BitVec::zero(w);
    v.as_mut().into_words().copy_from_slice(a);
    v
}

/// 幅も選ぶ 4 値の値。
pub fn any_logic_vec() -> impl Strategy<Value = LogicVec> {
    width().prop_flat_map(logic_vec)
}

/// 幅も選ぶ 2 値の値。
pub fn any_bit_vec() -> impl Strategy<Value = BitVec> {
    width().prop_flat_map(bit_vec)
}

/// 同じ幅の 4 値の 2 つ組 (文脈決定の 2 項演算用)。
pub fn logic_pair() -> impl Strategy<Value = (LogicVec, LogicVec)> {
    width().prop_flat_map(|w| (logic_vec(w), logic_vec(w)))
}

/// 同じ幅の 4 値の 2 つ組 (算術向け)。
pub fn logic_pair_arith() -> impl Strategy<Value = (LogicVec, LogicVec)> {
    width().prop_flat_map(|w| (logic_vec_arith(w), logic_vec_arith(w)))
}

/// 同じ幅の 4 値の 3 つ組 (`?:` など)。
pub fn logic_triple() -> impl Strategy<Value = (LogicVec, LogicVec, LogicVec)> {
    width().prop_flat_map(|w| (logic_vec(w), logic_vec(w), logic_vec(w)))
}

/// 同じ幅の 2 値の 2 つ組。
pub fn bit_pair() -> impl Strategy<Value = (BitVec, BitVec)> {
    width().prop_flat_map(|w| (bit_vec(w), bit_vec(w)))
}

/// 幅 ≤ 64 の、同じ幅の 4 値の 2 つ組 (層 1 用)。
pub fn scalar_logic_pair() -> impl Strategy<Value = (LogicVec, LogicVec)> {
    scalar_width().prop_flat_map(|w| (logic_vec(w), logic_vec(w)))
}

// ===========================================================================
// 自己決定のオペランド
// ===========================================================================

/// シフト量の幅の候補 (シフト量は自己決定で、データと別の幅を持つ)。
const AMOUNT_WIDTHS: [u32; 9] = [1, 3, 5, 8, 16, 32, 64, 65, 128];

/// データ幅 `w` に対するシフト量 (plan.md §5.3): `0..=w+2`、64 の倍数の前後、64 ビットを超える
/// 巨大な値、X/Z を含む値。シフト量の幅は [`AMOUNT_WIDTHS`] から選び、値はその幅に切り詰める。
pub fn shift_amount(w: u32) -> BoxedStrategy<LogicVec> {
    let value = prop_oneof![
        4 => (0u64..=w as u64 + 2).prop_map(|v| v as u128),
        // 64 の倍数の前後 (k*64 - 2 ..= k*64 + 2)
        2 => (0i64..=(w as i64 / 64 + 1), -2i64..=2).prop_map(|(k, d)| (k * 64 + d).max(0) as u128),
        1 => any::<u128>(),
        1 => Just(u128::MAX),
        1 => Just(1u128 << 64),
    ];
    let amt_width = prop::sample::select(&AMOUNT_WIDTHS[..]);
    (
        amt_width,
        value,
        density(Profile::Arith),
        ratio(),
        any::<u64>(),
    )
        .prop_map(|(aw, v, d, r, seed)| {
            let mut a = vec![0u64; words_for(aw)];
            for (i, word) in a.iter_mut().enumerate().take(2) {
                *word = (v >> (64 * i)) as u64;
            }
            normalize(aw, &mut a);
            inject_unknowns(aw, &a, d, r, seed)
        })
        .boxed()
}

/// データ幅 `w` に対する 2 値のシフト量 (X/Z を含まない)。
pub fn shift_amount_bits(w: u32) -> BoxedStrategy<BitVec> {
    shift_amount(w).prop_map(|v| v.to_bit_vec()).boxed()
}

/// 選択のオフセット (plan.md §5.3): 負、0、範囲の境界、完全に範囲外、`i64` の端、未知。
///
/// `src_w` は選択元の幅、`sel_w` は選択幅。`None` は `Index::Unknown` (X/Z のインデックス) を表す。
/// LSB を 0 とする物理オフセット (plan.md §3.5(e))。
pub fn select_offset(src_w: u32, sel_w: u32) -> BoxedStrategy<Option<i64>> {
    let s = src_w as i64;
    let l = sel_w as i64;
    prop_oneof![
        4 => (0..=(s - l).max(0)).prop_map(Some),
        1 => Just(Some(0)),
        1 => Just(Some(s - l)),
        1 => Just(Some(s - l + 1)),
        1 => Just(Some(-1)),
        1 => Just(Some(-l)),
        1 => Just(Some(-l + 1)),
        1 => (-(l + 3)..0i64).prop_map(Some),
        1 => Just(Some(s)),
        1 => (s..s + 70).prop_map(Some),
        1 => Just(Some(i64::MIN)),
        1 => Just(Some(i64::MAX)),
        1 => Just(Some(i64::MAX - l + 1)),
        1 => Just(None),
    ]
    .boxed()
}

/// 幅 `w` の除数 (plan.md §5.3): 0、1、-1 (全 1)、2 のべき、乱数。X/Z は含まない。
pub fn divisor(w: u32) -> BoxedStrategy<LogicVec> {
    prop_oneof![
        2 => Just(LogicVec::zero(w)),
        1 => Just(LogicVec::from_u64(w, 1)),
        1 => Just(LogicVec::filled(w, Logic::L1)),
        2 => (0..w).prop_map(move |i| {
            let mut v = LogicVec::zero(w);
            v.set_bit(i, Logic::L1);
            v
        }),
        4 => logic_vec_known(w),
        2 => any::<u64>().prop_map(move |v| LogicVec::from_u64(w, v)),
    ]
    .boxed()
}

/// 幅 `w` の 2 値の除数。
pub fn divisor_bits(w: u32) -> BoxedStrategy<BitVec> {
    divisor(w).prop_map(|v| v.to_bit_vec()).boxed()
}

/// 指数の幅の候補。
const EXP_WIDTHS: [u32; 8] = [1, 2, 4, 8, 32, 64, 65, 100];

/// べき乗の指数 (plan.md §5.3): 負 (符号付きで解釈したとき)、0、1、大きい値。X/Z も少し含む。
/// 指数は自己決定なので、幅は [`EXP_WIDTHS`] から選ぶ。符号付きかどうかはテスト側が決める。
pub fn exponent() -> BoxedStrategy<LogicVec> {
    prop::sample::select(&EXP_WIDTHS[..])
        .prop_flat_map(|w| {
            prop_oneof![
                2 => Just(LogicVec::zero(w)),
                2 => Just(LogicVec::from_u64(w, 1)),
                1 => Just(LogicVec::from_u64(w, 2)),
                1 => Just(LogicVec::from_u64(w, 3)),
                // 符号付きなら -1
                2 => Just(LogicVec::filled(w, Logic::L1)),
                // 符号付きなら最小値 (負)
                1 => Just({
                    let mut v = LogicVec::zero(w);
                    v.set_bit(w - 1, Logic::L1);
                    v
                }),
                1 => (0u64..=8).prop_map(move |v| LogicVec::from_i64(w, -(v as i64))),
                3 => logic_vec_arith(w),
            ]
        })
        .boxed()
}

// ===========================================================================
// 参照モデルとの受け渡し (bits 文字列経由、plan.md §3.7)
// ===========================================================================

/// 本体の値を参照モデルの値に変換する。
pub fn to_ref(x: &LogicVec) -> RefVec {
    ref_from_bits(&x.to_bits_string())
}

/// ビューを参照モデルの値に変換する。
pub fn view_to_ref(x: LogicRef<'_>) -> RefVec {
    ref_from_bits(&x.to_bits_string())
}

/// 2 値の値を参照モデルの値に変換する。
pub fn bits_to_ref(x: &BitVec) -> RefVec {
    ref_from_bits(&x.to_bits_string())
}

fn ref_from_bits(s: &str) -> RefVec {
    RefVec::from_bits_str(s).unwrap_or_else(|e| panic!("RefVec::from_bits_str({s:?}): {e}"))
}

/// 参照モデルの値を本体の値に変換する。
pub fn from_ref(x: &RefVec) -> LogicVec {
    let s = x.to_bits_string();
    LogicVec::from_bits_str(&s).unwrap_or_else(|e| panic!("LogicVec::from_bits_str({s:?}): {e}"))
}

/// 参照モデルの値を 2 値の本体の値に変換する。X/Z を含めば panic する
/// (2 値版の期待値は「参照モデルの結果 → `to_two_state`」で作る。plan.md §5.2)。
pub fn from_ref_bits(x: &RefVec) -> BitVec {
    let s = x.to_bits_string();
    BitVec::from_bits_str(&s).unwrap_or_else(|e| panic!("BitVec::from_bits_str({s:?}): {e}"))
}

// ===========================================================================
// 検査
// ===========================================================================

/// 4 値の出力が正規化不変条件 (plan.md §3.2) を満たすことを検査する。
pub fn check_normalized(x: LogicRef<'_>, ctx: &str) -> Result<(), TestCaseError> {
    prop_assert_eq!(x.aval().len(), words_for(x.width()), "{}: aval length", ctx);
    prop_assert_eq!(x.bval().len(), words_for(x.width()), "{}: bval length", ctx);
    prop_assert!(
        x.is_normalized(),
        "{}: not normalized (width {}, aval {:x?}, bval {:x?})",
        ctx,
        x.width(),
        x.aval(),
        x.bval()
    );
    Ok(())
}

/// 2 値の出力が正規化不変条件を満たすことを検査する。
pub fn check_normalized2(x: BitRef<'_>, ctx: &str) -> Result<(), TestCaseError> {
    prop_assert_eq!(x.words().len(), words_for(x.width()), "{}: length", ctx);
    prop_assert!(
        x.is_normalized(),
        "{}: not normalized (width {}, words {:x?})",
        ctx,
        x.width(),
        x.words()
    );
    Ok(())
}

/// 層 1 の出力 (幅 `w`) が正規化されていることを検査する。
pub fn check_normalized_l64(x: L64, w: u32, ctx: &str) -> Result<(), TestCaseError> {
    prop_assert!(
        x.is_normalized(w),
        "{}: L64 not normalized for width {} ({:x?})",
        ctx,
        w,
        x
    );
    Ok(())
}

/// 4 値の出力が参照モデルの結果と全ビット一致 (aval・bval とも) し、正規化されていることを検査する。
pub fn check_eq_ref(
    actual: LogicRef<'_>,
    expected: &RefVec,
    ctx: &str,
) -> Result<(), TestCaseError> {
    check_normalized(actual, ctx)?;
    prop_assert_eq!(
        actual.width() as usize,
        expected.width(),
        "{}: width mismatch",
        ctx
    );
    let got = actual.to_bits_string();
    let want = expected.to_bits_string();
    prop_assert_eq!(got, want, "{}: value mismatch", ctx);
    Ok(())
}

/// 2 値の出力が参照モデルの結果と一致し、正規化されていることを検査する。
/// `expected` は X/Z を含まないこと (含む場合は先に `to_two_state` すること)。
pub fn check_eq_ref2(
    actual: BitRef<'_>,
    expected: &RefVec,
    ctx: &str,
) -> Result<(), TestCaseError> {
    check_normalized2(actual, ctx)?;
    prop_assert_eq!(
        actual.width() as usize,
        expected.width(),
        "{}: width mismatch",
        ctx
    );
    prop_assert_eq!(
        actual.to_bits_string(),
        expected.to_bits_string(),
        "{}: value mismatch",
        ctx
    );
    Ok(())
}

/// 層 1 の出力 (幅 `w`) が参照モデルの結果と一致し、正規化されていることを検査する。
pub fn check_eq_ref_l64(
    actual: L64,
    w: u32,
    expected: &RefVec,
    ctx: &str,
) -> Result<(), TestCaseError> {
    check_normalized_l64(actual, w, ctx)?;
    let v = from_l64(actual, w);
    check_eq_ref(v.as_ref(), expected, ctx)
}

/// 2 つの 4 値の出力 (例: 層 2 と層 3、`_assign` 版と通常版) が全ビット一致することを検査する。
pub fn check_eq(
    actual: LogicRef<'_>,
    expected: LogicRef<'_>,
    ctx: &str,
) -> Result<(), TestCaseError> {
    check_normalized(actual, ctx)?;
    prop_assert_eq!(actual.width(), expected.width(), "{}: width mismatch", ctx);
    prop_assert_eq!(actual.aval(), expected.aval(), "{}: aval mismatch", ctx);
    prop_assert_eq!(actual.bval(), expected.bval(), "{}: bval mismatch", ctx);
    Ok(())
}

/// 2 つの 2 値の出力が一致することを検査する。
pub fn check_eq2(actual: BitRef<'_>, expected: BitRef<'_>, ctx: &str) -> Result<(), TestCaseError> {
    check_normalized2(actual, ctx)?;
    prop_assert_eq!(actual.width(), expected.width(), "{}: width mismatch", ctx);
    prop_assert_eq!(actual.words(), expected.words(), "{}: value mismatch", ctx);
    Ok(())
}

/// 1 ビットの結果 ([`Logic`]) が参照モデルの結果 (幅 1 の `RefVec`) と一致することを検査する。
pub fn check_logic_eq_ref(
    actual: Logic,
    expected: &RefVec,
    ctx: &str,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        expected.width(),
        1,
        "{}: reference result must be 1 bit",
        ctx
    );
    prop_assert_eq!(
        actual.to_char().to_string(),
        expected.to_bits_string(),
        "{}: value mismatch",
        ctx
    );
    Ok(())
}

/// [`Logic`] を参照モデルの [`RefLogic`] にする (bits 文字列の 1 文字経由。§6.3.1 の 0/1/x/z)。
pub fn logic_to_ref(x: Logic) -> RefLogic {
    RefLogic::from_char(x.to_char()).expect("Logic::to_char is one of 0/1/x/z")
}

/// 参照モデルの [`RefLogic`] を [`Logic`] にする。
pub fn logic_from_ref(x: RefLogic) -> Logic {
    Logic::from_char(x.to_char()).expect("RefLogic::to_char is one of 0/1/x/z")
}

/// 1 ビットの結果 ([`Logic`]) が参照モデルの 1 ビットの結果 ([`RefLogic`]) と一致することを検査する
/// (`RefVec::bit` や、`RefLogic` を返す参照モデルの関数の結果と比べるとき)。
pub fn check_logic_eq_reflogic(
    actual: Logic,
    expected: RefLogic,
    ctx: &str,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        actual.to_char(),
        expected.to_char(),
        "{}: value mismatch (actual {:?}, expected {:?})",
        ctx,
        actual,
        expected
    );
    Ok(())
}

/// 2 値の 1 ビットの結果 (`bool`) が期待値と一致することを検査する。
///
/// 2 値版の期待値は「参照モデルの結果 → `to_two_state`」で作る (plan.md §5.2)。参照モデルの
/// 1 ビットの結果から期待値を作るときは [`ref_to_two_state_bool`] を使う。
pub fn check_bool_eq_ref(actual: bool, expected: bool, ctx: &str) -> Result<(), TestCaseError> {
    prop_assert_eq!(actual, expected, "{}: value mismatch", ctx);
    Ok(())
}

/// 参照モデルの 1 ビットの結果を 2 値化する (1 だけが真、0/x/z は偽。§6.11.2)。
pub fn ref_to_two_state_bool(x: RefLogic) -> bool {
    x == RefLogic::One
}

/// 列挙型の結果 (例: エッジ検出の `Edge`、§9.4.2) が参照モデルの結果 (例: `RefEdge`) と一致することを
/// 検査する。
///
/// 本体と参照モデルの型は互いに依存しないので、**`Debug` の表示 (バリアント名) が同じ**ことで比べる
/// (例: `Edge::Pos` と `RefEdge::Pos` はどちらも `"Pos"`)。バリアント名が対応しない型には使えない。
/// その場合は [`check_value_eq`] に、期待値を本体の型に写したものを渡す。
pub fn check_edge_eq_ref<A: core::fmt::Debug, E: core::fmt::Debug>(
    actual: A,
    expected: E,
    ctx: &str,
) -> Result<(), TestCaseError> {
    let (a, e) = (format!("{actual:?}"), format!("{expected:?}"));
    prop_assert_eq!(a, e, "{}: value mismatch", ctx);
    Ok(())
}

/// 同じ型の 2 つの値 (`PartialEq + Debug`) が一致することを検査する汎用の補助。
pub fn check_value_eq<T: PartialEq + core::fmt::Debug>(
    actual: T,
    expected: T,
    ctx: &str,
) -> Result<(), TestCaseError> {
    prop_assert_eq!(actual, expected, "{}: value mismatch", ctx);
    Ok(())
}

/// `bool` を参照モデルの 1 ビットの値にする (真 → 1、偽 → 0)。
pub fn bool_to_ref(b: bool) -> RefLogic {
    RefLogic::from_bool(b)
}

/// エッジ検出の結果 ([`Edge`]、§9.4.2) を参照モデルの [`RefEdge`] にする。
pub fn edge_to_ref(e: Edge) -> RefEdge {
    match e {
        Edge::None => RefEdge::None,
        Edge::Pos => RefEdge::Pos,
        Edge::Neg => RefEdge::Neg,
    }
}

/// 2 値版の期待値: 参照モデルの結果 → `to_two_state` (CR-5、plan.md §5.2)。X/Z は 0 になる。
pub fn ref_two_state(x: &RefVec) -> RefVec {
    svsim_logic_ref::to_two_state(x)
}

/// 層 1 の 2 値の結果 (幅 `w` の `u64`) が正規化され、参照モデルの結果と一致することを検査する。
pub fn check_eq_ref_u64(got: u64, w: u32, want: &RefVec, ctx: &str) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        got & !svsim_logic::scalar::mask(w),
        0,
        "{}: u64 not normalized",
        ctx
    );
    check_eq_ref2(BitVec::from_u64(w, got).as_ref(), want, ctx)
}

/// 入力が演算の前後で変わっていないことを検査する (CR-7)。
pub fn check_unchanged<T: PartialEq + core::fmt::Debug>(
    before: &T,
    after: &T,
    ctx: &str,
) -> Result<(), TestCaseError> {
    prop_assert!(
        before == after,
        "{}: input modified ({:?} -> {:?})",
        ctx,
        before,
        after
    );
    Ok(())
}

/// proptest の外 (網羅テスト) で検査関数の結果を確かめる。`Err` なら panic する。
pub fn run_case(r: Result<(), TestCaseError>) {
    if let Err(e) = r {
        panic!("{e}");
    }
}

// ===========================================================================
// 網羅テストの入力 (plan.md §5.4)
// ===========================================================================

/// 幅 `w` の 4 値の全値 (4^w 個)。ビット `i` は `n` の 4 進の `i` 桁目 (0, 1, x, z の順)。
pub fn all_logic(w: u32) -> Vec<LogicVec> {
    (0..4usize.pow(w))
        .map(|mut n| {
            let mut v = LogicVec::zero(w);
            for i in 0..w {
                v.set_bit(i, Logic::ALL[n % 4]);
                n /= 4;
            }
            v
        })
        .collect()
}

/// 幅 `w` の 2 値の全値 (2^w 個)。
pub fn all_bits(w: u32) -> Vec<BitVec> {
    (0..1u64 << w).map(|n| BitVec::from_u64(w, n)).collect()
}

// ===========================================================================
// M0-TC52 幅境界の横断テストの入力 (L-5)
// ===========================================================================

/// TC52 の固定幅 (乱数に頼らない境界の幅と、幅 > 1000)。
pub const TC52_WIDTHS: [u32; 16] = [
    1, 2, 31, 32, 33, 63, 64, 65, 127, 128, 129, 1000, 1023, 1024, 1025, 4096,
];

/// TC52 の値の模様 (幅 `w` ≥ 1): 全 0、全 1、`1`、MSB だけ 1、MSB 以外全 1、交互 `0101…`、
/// bit 63/64 の両側だけ 1 (幅に収まるものだけ)、全 x、全 z、MSB だけ x、LSB だけ z。
///
/// 幅が小さいと同じ値が重なる (例: 幅 1 の全 1 と `1`)。重なりはそのまま残す (件数が少し増えるだけ)。
pub fn tc52_patterns(w: u32) -> Vec<LogicVec> {
    let mut out = vec![
        LogicVec::zero(w),
        LogicVec::filled(w, Logic::L1),
        LogicVec::from_u64(w, 1),
    ];
    let mut msb = LogicVec::zero(w);
    msb.set_bit(w - 1, Logic::L1);
    out.push(msb);
    let mut not_msb = LogicVec::filled(w, Logic::L1);
    not_msb.set_bit(w - 1, Logic::L0);
    out.push(not_msb);
    let mut alt = LogicVec::zero(w);
    for i in (0..w).step_by(2) {
        alt.set_bit(i, Logic::L1);
    }
    out.push(alt);
    let mut b63 = LogicVec::zero(w);
    for i in [63u32, 64] {
        if i < w {
            b63.set_bit(i, Logic::L1);
        }
    }
    out.push(b63);
    out.push(LogicVec::filled(w, Logic::X));
    out.push(LogicVec::filled(w, Logic::Z));
    let mut msb_x = LogicVec::zero(w);
    msb_x.set_bit(w - 1, Logic::X);
    out.push(msb_x);
    let mut lsb_z = LogicVec::zero(w);
    lsb_z.set_bit(0, Logic::Z);
    out.push(lsb_z);
    out
}

/// TC52 の模様の 2 値版 (X/Z を 0 にしたもの。重複は除く)。
pub fn tc52_bit_patterns(w: u32) -> Vec<BitVec> {
    let mut out: Vec<BitVec> = Vec::new();
    for p in tc52_patterns(w) {
        let b = p.to_bit_vec();
        if !out.contains(&b) {
            out.push(b);
        }
    }
    out
}

// ===========================================================================
// 層 1 / 層 2 のための補助
// ===========================================================================

/// 幅 ≤ 64 の値を層 1 の値にする。
pub fn to_l64(x: &LogicVec) -> L64 {
    L64::load(x.as_ref())
}

/// 層 1 の値 (幅 `w`) を owned 値にする。正規化されていなくてもそのまま写すので、
/// 検査には [`check_normalized_l64`] を先に使うこと。
pub fn from_l64(x: L64, w: u32) -> LogicVec {
    assert!(w <= 64);
    let mut v = LogicVec::zero(w);
    if w > 0 {
        let (a, b) = v.as_mut().into_planes();
        a[0] = x.a;
        b[0] = x.b;
    }
    v
}

/// 幅 ≤ 64 の 2 値の値を層 1 の値にする。
pub fn to_u64_scalar(x: &BitVec) -> u64 {
    svsim_logic::scalar::load2(x.as_ref())
}

/// aval と bval を別々の `Vec` に持つ 4 値 (プレーンが隣接しないビューを作るため)。
///
/// 層 2 のカーネルの出力先には [`SplitLogic::dirty`] を使う。全ビットが X で、幅より上のビットも
/// 1 で埋まっているので、カーネルが出力の全ビットを書き、正規化することを検査できる。
#[derive(Clone, Debug)]
pub struct SplitLogic {
    /// ビット幅。
    pub width: u32,
    /// aval プレーン。
    pub a: Vec<u64>,
    /// bval プレーン。
    pub b: Vec<u64>,
}

impl SplitLogic {
    /// 値を写して作る。
    pub fn from_vec(x: &LogicVec) -> Self {
        let r = x.as_ref();
        SplitLogic {
            width: x.width(),
            a: r.aval().to_vec(),
            b: r.bval().to_vec(),
        }
    }

    /// 全ビット X で埋め、さらに**幅より上のビット (最上位ワードの未使用ビット) も 1 にした**出力先
    /// (正規化されていない)。カーネルが出力の全ビットを書くことと、最後に正規化すること
    /// (plan.md §3.2、test-plan TC12) を同時に検査できる。
    pub fn dirty(width: u32) -> Self {
        let nw = words_for(width);
        SplitLogic {
            width,
            a: vec![!0u64; nw],
            b: vec![!0u64; nw],
        }
    }

    /// 読み取りビュー。
    pub fn as_ref(&self) -> LogicRef<'_> {
        LogicRef::new(self.width, &self.a, &self.b)
    }

    /// 書き込みビュー。
    pub fn as_mut(&mut self) -> LogicMut<'_> {
        LogicMut::new(self.width, &mut self.a, &mut self.b)
    }

    /// owned 値にする。
    pub fn to_vec(&self) -> LogicVec {
        LogicVec::from_ref(self.as_ref())
    }
}

/// 2 値の出力先 (プレーンを別の `Vec` に持つ)。
#[derive(Clone, Debug)]
pub struct SplitBits {
    /// ビット幅。
    pub width: u32,
    /// ワード列。
    pub a: Vec<u64>,
}

impl SplitBits {
    /// 値を写して作る。
    pub fn from_vec(x: &BitVec) -> Self {
        SplitBits {
            width: x.width(),
            a: x.as_ref().words().to_vec(),
        }
    }

    /// 全ビット 1 で埋め、さらに幅より上のビットも 1 にした出力先 (正規化されていない)。
    /// [`SplitLogic::dirty`] と同じ目的。
    pub fn dirty(width: u32) -> Self {
        SplitBits {
            width,
            a: vec![!0u64; words_for(width)],
        }
    }

    /// 読み取りビュー。
    pub fn as_ref(&self) -> BitRef<'_> {
        BitRef::new(self.width, &self.a)
    }

    /// 書き込みビュー。
    pub fn as_mut(&mut self) -> BitMut<'_> {
        BitMut::new(self.width, &mut self.a)
    }

    /// owned 値にする。
    pub fn to_vec(&self) -> BitVec {
        BitVec::from_ref(self.as_ref())
    }
}
