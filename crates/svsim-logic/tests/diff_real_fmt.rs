//! 実数との変換と表示用フォーマットの、参照モデルとの差分テスト (plan.md §5.2, §5.3、test-plan
//! TC44, TC48 (性質), TC49, TC54 (補助の性質), TC52 (幅の境界))。
//!
//! - 実数: `to_f64 to_f32 from_f64_round from_f64_trunc` (4 値・2 値) の値を参照モデルと比べる。
//!   参照モデルの `RefRealToIntStatus` はまだ 2 種 (Ok/NotFinite) なので、状態は参照モデルと比べず、
//!   test-plan TC43 の定義 (Q-6) から独立に求めたものと比べる (TC44 の「性質 (状態)」)。
//! - 表示: `format_int` / `format_int2` / `itoa` を参照モデルの `format_int` / `itoa` と文字列で比べる。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::fmt::{self, FieldWidth, FmtRadix};
use svsim_logic::real::{self, RealToIntStatus};
use svsim_logic::{BitVec, LogicVec, Scratch};
use svsim_logic_ref as r;
use svsim_logic_ref::{RefFieldWidth, RefFmtRadix, RefRealToIntStatus, RefVec};

// ===========================================================================
// ローカルの補助 (tests/common は読むだけ、plan.md §3.7)
// ===========================================================================

/// どの有限の f64 の整数値も符号付きで収まる幅 (2^1024 未満 → 1025 ビットで足りる)。
const WIDE: u32 = 1100;

/// f64 の入力 (TC44): ビットパターンの一様乱数、非正規化数、±0、±∞、NaN、2 のべき ± 0.5、
/// 整数 + 0.5、整数に近い値。
fn f64_value() -> BoxedStrategy<f64> {
    let special = prop::sample::select(vec![
        0.0,
        -0.0,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::MIN,
        f64::EPSILON,
        5e-324,
        -5e-324,
        0.5,
        -0.5,
        0.49999999999999994,
        1.4999999999999998,
        4503599627370495.5,
        1e20,
        1e300,
    ]);
    let pow2_half = (-60i32..1024, any::<bool>(), 0u8..3).prop_map(|(k, neg, d)| {
        let p = 2f64.powi(k);
        let v = match d {
            0 => p,
            1 => p + 0.5,
            _ => p - 0.5,
        };
        if neg { -v } else { v }
    });
    let half = (any::<i64>(), any::<bool>()).prop_map(|(n, neg)| {
        let v = (n >> (n as u64 % 60)) as f64 + 0.5;
        if neg { -v } else { v }
    });
    let near_int = (any::<i32>(), -3i64..=3).prop_map(|(n, ulps)| {
        let b = (n as f64).to_bits() as i64;
        f64::from_bits((b + ulps) as u64)
    });
    let subnormal = (1u64..(1 << 52), any::<bool>())
        .prop_map(|(m, neg)| f64::from_bits(m | (u64::from(neg) << 63)));
    let bits = any::<u64>().prop_map(f64::from_bits);
    prop_oneof![
        3 => special,
        3 => pow2_half,
        2 => half,
        2 => near_int,
        1 => subnormal,
        4 => bits,
    ]
    .boxed()
}

/// 丸めの境界の整数 (TC44、R-12): 仮数 `p` ビット (f64 は 53、f32 は 24) の後ろに、ちょうど半分、
/// 半分 ± sticky、半分の手前を置く。2^precision への丸め上げ、±∞ への溢れの境界も含む。
fn rounding_boundary() -> BoxedStrategy<(LogicVec, bool)> {
    (
        prop::sample::select(vec![24u32, 25, 53, 54]),
        any::<u64>(),
        0u32..1100,
        0u8..5,
        any::<bool>(),
    )
        .prop_map(|(p, mant, shift, tail, signed)| {
            let shift = shift.max(2);
            let w = (shift + p + 2).min(2100);
            let mut x = LogicVec::zero(w);
            // 仮数 (最上位ビットは 1)。tail == 4 は全部 1 (丸め上げで桁が上がる)。
            let m = if tail == 4 {
                u64::MAX
            } else {
                mant | (1 << 63)
            };
            for i in 0..p {
                if (m >> (63 - i)) & 1 != 0 {
                    x.set_bit(shift + p - 1 - i, svsim_logic::Logic::L1);
                }
            }
            match tail {
                0 => x.set_bit(shift - 1, svsim_logic::Logic::L1), // ちょうど半分
                1 => {
                    x.set_bit(shift - 1, svsim_logic::Logic::L1);
                    x.set_bit(0, svsim_logic::Logic::L1); // 半分 + sticky
                }
                2 => {
                    for i in 0..shift - 1 {
                        x.set_bit(i, svsim_logic::Logic::L1); // 半分の手前
                    }
                }
                _ => x.set_bit(shift - 1, svsim_logic::Logic::L1),
            }
            (x, signed)
        })
        .boxed()
}

/// 実数 → 整数の dst の幅: 通常の幅の戦略と、どの f64 も収まる広い幅。
fn real_dst_width() -> BoxedStrategy<u32> {
    prop_oneof![8 => width(), 1 => Just(WIDE), 1 => Just(1025u32)].boxed()
}

fn ref_radix(r: FmtRadix) -> RefFmtRadix {
    match r {
        FmtRadix::Bin => RefFmtRadix::Bin,
        FmtRadix::Oct => RefFmtRadix::Oct,
        FmtRadix::Dec => RefFmtRadix::Dec,
        FmtRadix::Hex => RefFmtRadix::Hex,
    }
}

fn ref_width(w: FieldWidth) -> RefFieldWidth {
    match w {
        FieldWidth::Auto => RefFieldWidth::Auto,
        FieldWidth::Min { width, zero_pad } => RefFieldWidth::Min { width, zero_pad },
    }
}

fn radix() -> impl Strategy<Value = FmtRadix> + Clone {
    prop::sample::select(vec![
        FmtRadix::Bin,
        FmtRadix::Oct,
        FmtRadix::Dec,
        FmtRadix::Hex,
    ])
}

/// TC49 の `FieldWidth`: `Auto`、`Min{0}`、`Min{n, zp}` (n ∈ 1..=40)。
fn field_width() -> impl Strategy<Value = FieldWidth> + Clone {
    prop_oneof![
        Just(FieldWidth::Auto),
        any::<bool>().prop_map(|zero_pad| FieldWidth::Min { width: 0, zero_pad }),
        (1u32..=40, any::<bool>())
            .prop_map(|(width, zero_pad)| FieldWidth::Min { width, zero_pad }),
    ]
}

/// 桁の中で x と z が混ざりやすい 4 値 (密度 50% / 全部、X と Z はビットごとに半々)。
fn mixed_xz(w: u32) -> BoxedStrategy<LogicVec> {
    let d = prop::sample::select(vec![UnknownDensity::Half, UnknownDensity::All]);
    (known_words(w), d, any::<u64>())
        .prop_map(move |(a, d, seed)| inject_unknowns(w, &a, d, XzRatio::Mixed, seed))
        .boxed()
}

/// 表示向けの 4 値: 桁の中で x と z が混ざるもの (`Mixed`) を多めにする。
fn fmt_logic_vec(w: u32) -> BoxedStrategy<LogicVec> {
    prop_oneof![
        2 => logic_vec(w),
        1 => mixed_xz(w),
        1 => logic_vec_known(w),
    ]
    .boxed()
}

/// f64 の一致 (ビット単位。NaN は「どちらも NaN」。整数からの変換では NaN は出ない)。
fn check_f64(got: f64, want: f64, ctx: &str) -> Result<(), TestCaseError> {
    if got.is_nan() || want.is_nan() {
        prop_assert!(
            got.is_nan() && want.is_nan(),
            "{}: {} vs {}",
            ctx,
            got,
            want
        );
    } else {
        prop_assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "{}: {} vs {}",
            ctx,
            got,
            want
        );
    }
    Ok(())
}

fn check_f32(got: f32, want: f32, ctx: &str) -> Result<(), TestCaseError> {
    if got.is_nan() || want.is_nan() {
        prop_assert!(
            got.is_nan() && want.is_nan(),
            "{}: {} vs {}",
            ctx,
            got,
            want
        );
    } else {
        prop_assert_eq!(
            got.to_bits(),
            want.to_bits(),
            "{}: {} vs {}",
            ctx,
            got,
            want
        );
    }
    Ok(())
}

/// TC43 の定義 (Q-6) から独立に求めた状態。`wide` は丸めた整数を幅 [`WIDE`] の符号付きで
/// 表したもの (参照モデルの結果)。収まるとは、幅 `w` に切り詰めて符号付きか符号なしで
/// 広げ直したときに元に戻ること (`-2^(w-1) <= n <= 2^w - 1`)。
fn expected_status(v: f64, w: u32, wide: &RefVec) -> RealToIntStatus {
    if !v.is_finite() {
        return RealToIntStatus::NotFinite;
    }
    let fits = if w == 0 {
        *wide == RefVec::zeros(WIDE as usize)
    } else {
        let t = r::resize(w as usize, wide, false);
        r::resize(WIDE as usize, &t, true) == *wide || r::resize(WIDE as usize, &t, false) == *wide
    };
    // 丸めでも切り捨てでも、小数部があれば Rounded (STATUS の決定)。
    let inexact = v.trunc() != v;
    if !fits {
        RealToIntStatus::Wrapped
    } else if inexact {
        RealToIntStatus::Rounded
    } else {
        RealToIntStatus::Exact
    }
}

// ===========================================================================
// 実数 (TC44)
// ===========================================================================

fn check_to_real(x: &LogicVec, signed: bool) -> Result<(), TestCaseError> {
    let ctx = format!("to_f64/to_f32 w={} signed={signed} x={x:?}", x.width());
    let rx = to_ref(x);
    let want64 = r::to_f64(&rx, signed);
    let want32 = r::to_f32(&rx, signed);
    check_f64(real::to_f64(x.as_ref(), signed), want64, &ctx)?;
    check_f32(real::to_f32(x.as_ref(), signed), want32, &ctx)?;
    // 層 3 と、プレーンが隣接しないビュー。
    check_f64(x.to_f64(signed), want64, &ctx)?;
    let split = SplitLogic::from_vec(x);
    check_f64(real::to_f64(split.as_ref(), signed), want64, &ctx)?;
    check_f32(real::to_f32(split.as_ref(), signed), want32, &ctx)?;
    // 2 値版: X/Z を 0 にした値で同じ結果 (V-14)。
    let b = x.to_bit_vec();
    check_f64(real::to_f64_2(b.as_ref(), signed), want64, &ctx)?;
    check_f32(real::to_f32_2(b.as_ref(), signed), want32, &ctx)?;
    check_f32(b.to_f32(signed), want32, &ctx)?;
    Ok(())
}

fn check_from_real(w: u32, v: f64, round: bool) -> Result<(), TestCaseError> {
    let ctx = format!(
        "from_f64_{} w={w} v={v:e} ({:#x})",
        if round { "round" } else { "trunc" },
        v.to_bits()
    );
    let (want, ref_status) = if round {
        r::from_f64_round(w as usize, v)
    } else {
        r::from_f64_trunc(w as usize, v)
    };
    // 参照モデルの状態は 2 種。NotFinite だけ照合する (値は比べる)。
    prop_assert_eq!(
        ref_status == RefRealToIntStatus::NotFinite,
        !v.is_finite(),
        "{}",
        ctx
    );
    let wide = if round {
        r::from_f64_round(WIDE as usize, v).0
    } else {
        r::from_f64_trunc(WIDE as usize, v).0
    };
    let want_status = expected_status(v, w, &wide);

    // 層 2 (プレーンが隣接せず、出力先を汚しておく)。
    let mut d = SplitLogic::dirty(w);
    let s = if round {
        real::from_f64_round(d.as_mut(), v)
    } else {
        real::from_f64_trunc(d.as_mut(), v)
    };
    check_eq_ref(d.as_ref(), &want, &ctx)?;
    prop_assert_eq!(s, want_status, "{}: status", ctx);

    // 層 3。
    let (x, s3) = if round {
        LogicVec::from_f64_round(w, v)
    } else {
        LogicVec::from_f64_trunc(w, v)
    };
    check_eq_ref(x.as_ref(), &want, &ctx)?;
    prop_assert_eq!(s3, want_status, "{}: status (layer 3)", ctx);

    // 2 値版。
    let mut d2 = SplitBits::dirty(w);
    let s2 = if round {
        real::from_f64_round2(d2.as_mut(), v)
    } else {
        real::from_f64_trunc2(d2.as_mut(), v)
    };
    check_eq_ref2(d2.as_ref(), &want, &ctx)?;
    prop_assert_eq!(s2, want_status, "{}: status (2-state)", ctx);
    let (b, sb) = if round {
        BitVec::from_f64_round(w, v)
    } else {
        BitVec::from_f64_trunc(w, v)
    };
    check_eq_ref2(b.as_ref(), &want, &ctx)?;
    prop_assert_eq!(sb, want_status, "{}: status (2-state layer 3)", ctx);
    Ok(())
}

proptest! {
    /// TC44: 整数 → 実数 (幅 1〜1024、S/U、X/Z を少し混ぜる)。
    #[test]
    fn to_real_matches_ref(
        x in width().prop_flat_map(logic_vec_arith),
        signed in any::<bool>(),
    ) {
        check_to_real(&x, signed)?;
    }

    /// TC44: 幅 1000 超の整数 → 実数 (±∞ への溢れを含む)。
    #[test]
    fn to_real_wide_matches_ref(
        x in prop::sample::select(vec![1023u32, 1024, 1025, 1100, 2048]).prop_flat_map(logic_vec_arith),
        signed in any::<bool>(),
    ) {
        check_to_real(&x, signed)?;
    }

    /// TC44: 丸めの境界の整数 → 実数 (最近接偶数、二重丸めの検出。R-12)。
    #[test]
    fn to_real_rounding_boundary_matches_ref((x, signed) in rounding_boundary()) {
        check_to_real(&x, signed)?;
        // 符号付きで負になるように 1 ビット広げたもの。
        // 負の値は参照モデルの neg で作る (ops::arith は別タスク)。
        let wide = x.resize(x.width() + 1, false);
        let n = from_ref(&r::neg(&to_ref(&wide)));
        check_to_real(&n, true)?;
    }

    /// TC44: 実数 → 整数 (丸めと切り捨て)。値は参照モデル、状態は Q-6 の定義と比べる。
    #[test]
    fn from_real_matches_ref(w in real_dst_width(), v in f64_value(), round in any::<bool>()) {
        check_from_real(w, v, round)?;
    }

    /// TC44 の性質: 有限の v と十分広い dst で `to_f64(from_f64_trunc(v), S) == v.trunc()`。
    #[test]
    fn trunc_round_trip(v in f64_value()) {
        prop_assume!(v.is_finite());
        let (x, _) = LogicVec::from_f64_trunc(WIDE, v);
        let back = x.to_f64(true);
        prop_assert_eq!(back, v.trunc(), "v = {:e}", v);
    }

    /// TC44 の性質: |n| <= 2^53 の整数 n で `to_f64(n) as i64 == n`。
    #[test]
    fn small_int_exact(n in -(1i64 << 53)..=(1i64 << 53), w in 55u32..=200) {
        let x = LogicVec::from_i64(w, n);
        prop_assert_eq!(x.to_f64(true) as i64, n);
        let (y, s) = LogicVec::from_f64_round(w, n as f64);
        prop_assert_eq!(s, RealToIntStatus::Exact);
        prop_assert_eq!(y, x);
    }
}

// ===========================================================================
// 表示 (TC49, TC48)
// ===========================================================================

fn check_format(
    x: &LogicVec,
    radix: FmtRadix,
    signed: bool,
    width: FieldWidth,
) -> Result<(), TestCaseError> {
    let ctx = format!(
        "format_int w={} {radix:?} signed={signed} {width:?} x={x:?}",
        x.width()
    );
    let want = r::format_int(&to_ref(x), ref_radix(radix), signed, ref_width(width));
    let mut s = Scratch::new();
    // 既存の内容の後ろに足すこと。
    let mut out = String::from("<");
    fmt::format_int(&mut out, x.as_ref(), radix, signed, width, &mut s);
    prop_assert_eq!(&out[1..], want.as_str(), "{}", ctx);
    let split = SplitLogic::from_vec(x);
    let mut out = String::new();
    fmt::format_int(&mut out, split.as_ref(), radix, signed, width, &mut s);
    prop_assert_eq!(&out, &want, "{} (split)", ctx);
    prop_assert_eq!(
        x.format(radix, signed, width),
        want.clone(),
        "{} (layer 3)",
        ctx
    );

    // Q-9 の補助: 長さの上限。
    if matches!(width, FieldWidth::Auto | FieldWidth::Min { width: 0, .. }) {
        let bound = fmt::format_len_bound(x.width(), radix);
        prop_assert!(
            want.len() <= bound,
            "{}: len {} > bound {}",
            ctx,
            want.len(),
            bound
        );
    }
    // 性質: Auto の文字数は値に依らない (TC49)。
    if width == FieldWidth::Auto {
        let zero = LogicVec::zero(x.width());
        prop_assert_eq!(
            want.len(),
            zero.format(radix, signed, width).len(),
            "{}: auto length",
            ctx
        );
    }
    // TC48: itoa == format_int(Min{0}) のバイト列、かつ参照モデルの itoa と一致。
    if width
        == (FieldWidth::Min {
            width: 0,
            zero_pad: false,
        })
    {
        let got = fmt::itoa(x.as_ref(), signed, radix, &mut s);
        prop_assert_eq!(&got, want.as_bytes(), "{} (itoa)", ctx);
        prop_assert_eq!(
            got,
            r::itoa(&to_ref(x), signed, ref_radix(radix)),
            "{} (ref itoa)",
            ctx
        );
    }

    // 2 値版: X/Z なしの入力で一致。
    if !x.has_unknown() {
        let b = x.to_bit_vec();
        let mut out = String::new();
        fmt::format_int2(&mut out, b.as_ref(), radix, signed, width, &mut s);
        prop_assert_eq!(&out, &want, "{} (2-state)", ctx);
        prop_assert_eq!(
            b.format(radix, signed, width),
            want.clone(),
            "{} (2-state layer 3)",
            ctx
        );
        if width
            == (FieldWidth::Min {
                width: 0,
                zero_pad: false,
            })
        {
            prop_assert_eq!(
                fmt::itoa2(b.as_ref(), signed, radix, &mut s),
                want.into_bytes()
            );
        }
    }
    Ok(())
}

proptest! {
    /// TC49: 幅 1〜1024、X/Z の全密度、4 種の基数 × S/U × FieldWidth。
    #[test]
    fn format_matches_ref(
        x in width().prop_flat_map(fmt_logic_vec),
        radix in radix(),
        signed in any::<bool>(),
        fw in field_width(),
    ) {
        check_format(&x, radix, signed, fw)?;
    }

    /// TC54 の補助 (P): `format_len_bound` は符号付きの最小値と全 x の表示の長さ以上。
    #[test]
    fn len_bound_covers_extremes(w in width(), radix in radix()) {
        let mut min = LogicVec::zero(w);
        min.set_bit(w - 1, svsim_logic::Logic::L1);
        let all_x = LogicVec::filled(w, svsim_logic::Logic::X);
        let ones = LogicVec::filled(w, svsim_logic::Logic::L1);
        let bound = fmt::format_len_bound(w, radix);
        for v in [&min, &all_x, &ones] {
            for signed in [false, true] {
                for fw in [FieldWidth::Auto, FieldWidth::Min { width: 0, zero_pad: false }] {
                    let s = v.format(radix, signed, fw);
                    prop_assert!(s.len() <= bound, "w={} {:?} {:?}: {} > {}", w, radix, fw, s.len(), bound);
                }
            }
        }
    }
}

/// TC52: 幅の境界で、決まった値の模様を全基数・全幅指定で比べる。
#[test]
fn format_and_real_boundary_widths() {
    use svsim_logic::Logic;
    let widths = [
        1u32, 2, 31, 32, 33, 63, 64, 65, 127, 128, 129, 1000, 1023, 1024, 1025, 4096,
    ];
    for &w in &widths {
        let mut pats = vec![
            LogicVec::zero(w),
            LogicVec::filled(w, Logic::L1),
            LogicVec::from_u64(w, 1),
            LogicVec::filled(w, Logic::X),
            LogicVec::filled(w, Logic::Z),
        ];
        let mut msb = LogicVec::zero(w);
        msb.set_bit(w - 1, Logic::L1);
        pats.push(msb);
        let mut not_msb = LogicVec::filled(w, Logic::L1);
        not_msb.set_bit(w - 1, Logic::L0);
        pats.push(not_msb);
        let mut alt = LogicVec::zero(w);
        for i in (0..w).step_by(2) {
            alt.set_bit(i, Logic::L1);
        }
        pats.push(alt);
        if w > 64 {
            let mut b = LogicVec::zero(w);
            b.set_bit(63, Logic::L1);
            b.set_bit(64, Logic::L1);
            pats.push(b);
        }
        let mut mx = LogicVec::zero(w);
        mx.set_bit(w - 1, Logic::X);
        pats.push(mx);
        let mut lz = LogicVec::zero(w);
        lz.set_bit(0, Logic::Z);
        pats.push(lz);
        for x in &pats {
            for radix in [FmtRadix::Bin, FmtRadix::Oct, FmtRadix::Dec, FmtRadix::Hex] {
                for signed in [false, true] {
                    for fw in [
                        FieldWidth::Auto,
                        FieldWidth::Min {
                            width: 0,
                            zero_pad: false,
                        },
                        FieldWidth::Min {
                            width: 7,
                            zero_pad: true,
                        },
                    ] {
                        check_format(x, radix, signed, fw).unwrap();
                    }
                }
            }
            for signed in [false, true] {
                check_to_real(x, signed).unwrap();
            }
        }
        for v in [
            0.5,
            -0.5,
            1e20,
            -1e20,
            2f64.powi(63),
            -(2f64.powi(63)),
            1e300,
            f64::NAN,
        ] {
            check_from_real(w, v, true).unwrap();
            check_from_real(w, v, false).unwrap();
        }
    }
}
