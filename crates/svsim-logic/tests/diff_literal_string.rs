//! リテラル解析・文字列変換・atoi 系の参照モデルとの差分テスト (plan.md §5.2, §5.3、test-plan TC39,
//! TC40, TC41 の性質)。
//!
//! - `parse_based` / `parse_unsized_decimal`: Ok/Err の別、値 (全ビット)、全フラグが参照モデルと一致
//!   (エラーの種類は参照モデルが文字列なので比べない)。
//! - `from_string_bytes` (層 2 は汚れた出力先、層 3、2 値版)、`to_string_bytes` (4 値・2 値、
//!   `write_*` 版)、`atoi` 系を参照モデルと比べる。
//! - 往復の性質: X/Z なしの値を 2/8/16/10 進の文字列にして `parse_based` に戻すと元に戻る (TC39)。
//!   0 を含まないバイト列は `from_string_bytes` → `to_string_bytes` で戻る (TC40)。
//!   `atoi` 系は `format!` の 10/16/8/2 進の文字列から戻る (TC41)。

mod common;

use common::*;
use proptest::prelude::*;
use proptest::test_runner::TestCaseError;
use svsim_logic::literal::{Literal, Radix, parse_based, parse_unsized_decimal};
use svsim_logic::string::{
    atoi, atoi_u32, from_string_bytes, from_string_bytes2, to_string_bytes, to_string_bytes2,
    write_string_bytes, write_string_bytes2,
};
use svsim_logic::{BitVec, LogicVec};
use svsim_logic_ref as r;
use svsim_logic_ref::{RefLiteral, RefRadix};

// ===========================================================================
// ローカルの補助 (tests/common は読むだけ、plan.md §3.7)
// ===========================================================================

fn ref_radix(x: Radix) -> RefRadix {
    match x {
        Radix::Bin => RefRadix::Bin,
        Radix::Oct => RefRadix::Oct,
        Radix::Dec => RefRadix::Dec,
        Radix::Hex => RefRadix::Hex,
    }
}

fn radix() -> impl Strategy<Value = Radix> {
    prop_oneof![
        Just(Radix::Bin),
        Just(Radix::Oct),
        Just(Radix::Dec),
        Just(Radix::Hex)
    ]
}

/// リテラルの桁の文字。各基数の数字 (多め)、`_`、x/z/?、ときどき範囲外の文字。
fn digit_char(radix: Radix) -> BoxedStrategy<char> {
    let digits: &'static [char] = match radix {
        Radix::Bin => &['0', '1'],
        Radix::Oct => &['0', '1', '2', '3', '4', '5', '6', '7'],
        Radix::Dec => &['0', '1', '2', '3', '4', '5', '6', '7', '8', '9'],
        Radix::Hex => &[
            '0', '1', '2', '3', '4', '5', '6', '7', '8', '9', 'a', 'b', 'c', 'd', 'e', 'f', 'A',
            'B', 'C', 'D', 'E', 'F',
        ],
    };
    prop_oneof![
        60 => proptest::sample::select(digits),
        10 => Just('_'),
        10 => proptest::sample::select(&['x', 'X', 'z', 'Z', '?'][..]),
        2 => proptest::sample::select(&['2', '8', '9', 'g', 'G', ' ', '-', '\'', 'é'][..]),
    ]
    .boxed()
}

/// 桁の並び。10 進では「単独の x/z/? + `_`」の形も混ぜる。長さは 0〜400。
fn digits(radix: Radix) -> BoxedStrategy<String> {
    let general = prop_oneof![
        3 => proptest::collection::vec(digit_char(radix), 1..=24),
        1 => proptest::collection::vec(digit_char(radix), 0..=400),
    ]
    .prop_map(|v| v.into_iter().collect::<String>());
    // 先頭が `_` でなく、x/z を含まない並び (Ok になりやすい)。
    let known = proptest::collection::vec(digit_char(radix), 1..=300).prop_map(|v| {
        let s: String = v
            .into_iter()
            .filter(|c| c.is_ascii_hexdigit() || *c == '_')
            .collect();
        s.trim_start_matches('_').to_string()
    });
    if radix == Radix::Dec {
        let unknown = (
            proptest::sample::select(&['x', 'X', 'z', 'Z', '?'][..]),
            0usize..3,
        )
            .prop_map(|(c, n)| format!("{c}{}", "_".repeat(n)));
        prop_oneof![4 => general, 3 => known, 1 => unknown].boxed()
    } else {
        prop_oneof![1 => general, 1 => known].boxed()
    }
}

fn size() -> impl Strategy<Value = Option<u32>> {
    prop_oneof![
        2 => Just(None),
        1 => Just(Some(0)),
        6 => width().prop_map(Some),
    ]
}

fn check_literal(got: &Literal, want: &RefLiteral, ctx: &str) -> Result<(), TestCaseError> {
    check_eq_ref(got.value.as_ref(), &want.value, ctx)?;
    prop_assert_eq!(got.signed, want.signed, "{}: signed", ctx);
    prop_assert_eq!(got.sized, want.sized, "{}: sized", ctx);
    prop_assert_eq!(got.xz_extend, want.xz_extend, "{}: xz_extend", ctx);
    prop_assert_eq!(got.truncated, want.truncated, "{}: truncated", ctx);
    prop_assert_eq!(
        got.widened_unsized,
        want.widened_unsized,
        "{}: widened_unsized",
        ctx
    );
    Ok(())
}

/// X/Z なしの値を、MSB が先頭の `radix` の文字列にする (先頭の 0 は付けない。`%0b` 等の形)。
fn format_known(v: &LogicVec, radix: Radix) -> String {
    let w = v.width() as usize;
    match radix.bits_per_digit() {
        Some(bpd) => {
            let bpd = bpd as usize;
            let nd = w.div_ceil(bpd);
            let mut s = String::new();
            for d in (0..nd).rev() {
                let mut x = 0u32;
                for k in 0..bpd {
                    let i = d * bpd + k;
                    if i < w && v.bit(i as u32) == svsim_logic::Logic::L1 {
                        x |= 1 << k;
                    }
                }
                s.push(char::from_digit(x, 16).unwrap());
            }
            let t = s.trim_start_matches('0');
            if t.is_empty() { "0".into() } else { t.into() }
        }
        None => {
            // 10 進: 10 で繰り返し割る (u128 で 1 ワードずつ)。
            let mut words: Vec<u64> = v.as_ref().aval().to_vec();
            let mut out = Vec::new();
            loop {
                let mut rem = 0u128;
                for w in words.iter_mut().rev() {
                    let t = (rem << 64) | u128::from(*w);
                    *w = (t / 10) as u64;
                    rem = t % 10;
                }
                out.push(b'0' + rem as u8);
                if words.iter().all(|&w| w == 0) {
                    break;
                }
            }
            out.reverse();
            String::from_utf8(out).unwrap()
        }
    }
}

// ===========================================================================
// §5.7.1 リテラル (TC39)
// ===========================================================================

proptest! {
    #[test]
    fn diff_parse_based(
        (radix, digits) in radix().prop_flat_map(|r| (Just(r), digits(r))),
        size in size(),
        signed in any::<bool>(),
    ) {
        let ctx = format!("parse_based({size:?}, {signed}, {radix:?}, {digits:?})");
        let got = parse_based(size, signed, radix, &digits);
        let want = r::parse_based(size, signed, ref_radix(radix), &digits);
        match (&got, &want) {
            (Ok(g), Ok(w)) => check_literal(g, w, &ctx)?,
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "{}: got {:?}, want {:?}", ctx, got, want),
        }
    }

    #[test]
    fn diff_parse_unsized_decimal(digits in digits(Radix::Dec)) {
        let ctx = format!("parse_unsized_decimal({digits:?})");
        let got = parse_unsized_decimal(&digits);
        let want = r::parse_unsized_decimal(&digits);
        match (&got, &want) {
            (Ok(g), Ok(w)) => check_literal(g, w, &ctx)?,
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "{}: got {:?}, want {:?}", ctx, got, want),
        }
    }

    /// 往復の性質 (TC39): X/Z なしの値 v (幅 w) の 2/8/16/10 進の文字列を `parse_based(Some(w))` に
    /// 戻すと v になり、truncated は立たない。サイズなしでも、幅 32 以上なら値が同じになる。
    #[test]
    fn prop_format_parse_round_trip(v in width().prop_flat_map(logic_vec_known), radix in radix()) {
        let w = v.width();
        let s = format_known(&v, radix);
        let ctx = format!("{w}'{radix:?} {s:?}");
        let l = parse_based(Some(w), false, radix, &s).map_err(|e| TestCaseError::fail(format!("{ctx}: {e}")))?;
        check_eq(l.value.as_ref(), v.as_ref(), &ctx)?;
        prop_assert!(!l.truncated, "{}: truncated", ctx);
        let u = parse_based(None, false, radix, &s).map_err(|e| TestCaseError::fail(format!("{ctx}: {e}")))?;
        prop_assert!(u.value.width() >= 32 && u.value.width() <= w.max(32), "{}: width {}", ctx, u.value.width());
        prop_assert_eq!(u.value.as_ref().aval()[0], v.as_ref().aval()[0], "{}: unsized low word", ctx);
    }
}

// ===========================================================================
// §5.9, §11.10.3, §6.16 文字列 ⇔ 整数 (TC40)
// ===========================================================================

fn bytes() -> impl Strategy<Value = Vec<u8>> {
    prop_oneof![
        3 => proptest::collection::vec(any::<u8>(), 0..=20),
        1 => proptest::collection::vec(any::<u8>(), 0..=160),
    ]
}

proptest! {
    #[test]
    fn diff_from_string_bytes(w in width(), s in bytes()) {
        let ctx = format!("from_string_bytes({w}, {s:x?})");
        let want = r::from_string_bytes(w as usize, &s);
        // 層 3
        let v = LogicVec::from_string_bytes(w, &s);
        check_eq_ref(v.as_ref(), &want, &ctx)?;
        // 層 2 (全 X で汚れた出力先、プレーンが隣接しない)
        let mut d = SplitLogic::dirty(w);
        from_string_bytes(d.as_mut(), &s);
        check_eq_ref(d.as_ref(), &want, &format!("{ctx} [slice]"))?;
        // 2 値 (全 1 で汚れた出力先)
        let want2 = r::to_two_state(&want);
        let b = BitVec::from_string_bytes(w, &s);
        check_eq_ref2(b.as_ref(), &want2, &format!("{ctx} [2]"))?;
        let mut d2 = SplitBits::dirty(w);
        from_string_bytes2(d2.as_mut(), &s);
        check_eq_ref2(d2.as_ref(), &want2, &format!("{ctx} [2 slice]"))?;
    }

    #[test]
    fn diff_to_string_bytes(x in any_logic_vec()) {
        let ctx = format!("to_string_bytes({x:?})");
        let want = r::to_string_bytes(&to_ref(&x));
        prop_assert_eq!(&to_string_bytes(x.as_ref()), &want, "{}", ctx);
        prop_assert_eq!(&x.to_string_bytes(), &want, "{} [vec]", ctx);
        let mut out = b"pre".to_vec();
        write_string_bytes(x.as_ref(), &mut out);
        prop_assert_eq!(&out[..3], b"pre");
        prop_assert_eq!(&out[3..], &want[..], "{} [write]", ctx);
    }

    #[test]
    fn diff_to_string_bytes2(x in any_bit_vec()) {
        let ctx = format!("to_string_bytes2({x:?})");
        let want = r::to_string_bytes(&bits_to_ref(&x));
        prop_assert_eq!(&to_string_bytes2(x.as_ref()), &want, "{}", ctx);
        prop_assert_eq!(&x.to_string_bytes(), &want, "{} [vec]", ctx);
        let mut out = Vec::new();
        write_string_bytes2(x.as_ref(), &mut out);
        prop_assert_eq!(&out, &want, "{} [write]", ctx);
    }

    /// 0 を含まないバイト列 s (長さ n) で、`to_string_bytes(from_string_bytes(8n, s)) == s` (TC40)。
    #[test]
    fn prop_string_round_trip(s in proptest::collection::vec(1u8..=255, 1..=200)) {
        let w = 8 * s.len() as u32;
        prop_assert_eq!(LogicVec::from_string_bytes(w, &s).to_string_bytes(), s.clone());
        prop_assert_eq!(BitVec::from_string_bytes(w, &s).to_string_bytes(), s);
    }
}

// ===========================================================================
// §6.16.9 atoi 系 (TC41)
// ===========================================================================

fn atoi_input() -> impl Strategy<Value = Vec<u8>> {
    let alphabet: &'static [u8] = b"0123456789abcdefABCDEFgxz_-+ \t";
    prop_oneof![
        4 => proptest::collection::vec(proptest::sample::select(alphabet), 0..=40),
        1 => proptest::collection::vec(any::<u8>(), 0..=40),
    ]
}

proptest! {
    #[test]
    fn diff_atoi(s in atoi_input(), radix in radix()) {
        let ctx = format!("atoi({:?}, {radix:?})", String::from_utf8_lossy(&s));
        let want = r::atoi(&s, ref_radix(radix));
        let got = atoi(&s, radix);
        check_eq_ref2(got.as_ref(), &want, &ctx)?;
        prop_assert_eq!(u64::from(atoi_u32(&s, radix)), got.to_u64().unwrap(), "{}", ctx);
    }

    /// 0〜2^32−1 の n で、`atoi(itoa(n)) = n` (`atohex/atooct/atobin` も同様。TC41)。
    #[test]
    fn prop_atoi_round_trip(n in any::<u32>()) {
        prop_assert_eq!(atoi_u32(format!("{n}").as_bytes(), Radix::Dec), n);
        prop_assert_eq!(atoi_u32(format!("{n:x}").as_bytes(), Radix::Hex), n);
        prop_assert_eq!(atoi_u32(format!("{n:X}").as_bytes(), Radix::Hex), n);
        prop_assert_eq!(atoi_u32(format!("{n:o}").as_bytes(), Radix::Oct), n);
        prop_assert_eq!(atoi_u32(format!("{n:b}").as_bytes(), Radix::Bin), n);
    }
}

// ===========================================================================
// 入力の分布の確認 (差分テストが意味のある場合を十分に試していること)
// ===========================================================================

/// 固定シードで `diff_parse_based` と同じ入力を作り、Ok/Err と各フラグの場合が両方とも出ることを
/// 確かめる。
#[test]
fn parse_based_input_coverage() {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;

    let strat = (
        radix().prop_flat_map(|r| (Just(r), digits(r))),
        size(),
        any::<bool>(),
    );
    let mut runner = TestRunner::deterministic();
    let (mut ok, mut err) = (0, 0);
    let (mut trunc, mut xz, mut widened, mut dec_unknown) = (0, 0, 0, 0);
    for _ in 0..4000 {
        let ((radix, digits), size, signed) = strat.new_tree(&mut runner).unwrap().current();
        match parse_based(size, signed, radix, &digits) {
            Ok(l) => {
                ok += 1;
                trunc += usize::from(l.truncated);
                xz += usize::from(l.xz_extend);
                widened += usize::from(l.widened_unsized);
                dec_unknown += usize::from(radix == Radix::Dec && l.value.has_unknown());
            }
            Err(_) => err += 1,
        }
    }
    eprintln!(
        "ok {ok}, err {err}, truncated {trunc}, xz {xz}, widened {widened}, dec x/z {dec_unknown}"
    );
    assert!(ok >= 1000 && err >= 400, "ok {ok}, err {err}");
    assert!(
        trunc >= 50 && xz >= 20 && widened >= 50 && dec_unknown >= 20,
        "truncated {trunc}, xz_extend {xz}, widened {widened}, dec x/z {dec_unknown}"
    );
}
