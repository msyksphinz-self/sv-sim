//! 算術のゴールデンテスト (IEEE 1800-2023 §11.4.2, §11.4.3、plan.md §3.5(c)、§3.5.1、
//! test-plan TC24〜TC27, TC34 の `pow_bits` の行)。
//!
//! 期待値は規格の規則と test-plan の表から固定値で書く。各行を層 3 で計算し、層 2 (プレーンが
//! 隣接しないビュー、事前確保した `Scratch`)・層 1 (幅 ≤ 64)・2 値版 (X/Z を含まない行) とも一致する
//! ことを確かめる。
//!
//! [要VCS確認 V-1] 符号付きの最小値 / -1 = 最小値、剰余 0。
//! [要VCS確認 V-2] 2 値の 0 除算・剰余と `0 ** 負` は 0。

use svsim_logic::ops::arith;
use svsim_logic::{BitMut, BitRef, BitVec, L64, Logic, LogicMut, LogicRef, LogicVec, Scratch};

// ===========================================================================
// 値の組み立て
// ===========================================================================

/// bits 文字列 (MSB が先頭、読みやすさのための `_` は取り除く)。
fn lv(s: &str) -> LogicVec {
    LogicVec::from_bits_str(&s.replace('_', "")).unwrap()
}

/// 16 進 (`_` 可、`x`/`z` の桁も可) を幅 `w` に切り詰めて (または 0 拡張して) 作る。
fn hx(w: u32, s: &str) -> LogicVec {
    let mut bits = String::new();
    for c in s.chars().filter(|&c| c != '_') {
        match c {
            'x' | 'X' => bits.push_str("xxxx"),
            'z' | 'Z' => bits.push_str("zzzz"),
            _ => bits.push_str(&format!("{:04b}", c.to_digit(16).unwrap())),
        }
    }
    let n = bits.len();
    let w = w as usize;
    let s = if n >= w {
        bits[n - w..].to_string()
    } else {
        "0".repeat(w - n) + &bits
    };
    LogicVec::from_bits_str(&s).unwrap()
}

fn u(w: u32, v: u64) -> LogicVec {
    LogicVec::from_u64(w, v)
}

fn i(w: u32, v: i64) -> LogicVec {
    LogicVec::from_i64(w, v)
}

/// ビット `k` だけ 1。
fn bit(w: u32, k: u32) -> LogicVec {
    let mut v = LogicVec::zero(w);
    v.set_bit(k, Logic::L1);
    v
}

fn ones(w: u32) -> LogicVec {
    LogicVec::filled(w, Logic::L1)
}

fn xs(w: u32) -> LogicVec {
    LogicVec::filled(w, Logic::X)
}

// ===========================================================================
// 層をまたいだ確認
// ===========================================================================

#[derive(Clone, Copy, Debug)]
enum Bin {
    Add,
    Sub,
    Mul,
    Div(bool),
    Rem(bool),
}

#[derive(Clone, Copy, Debug)]
enum Un {
    Neg,
    Inc,
    Dec,
}

/// プレーンを別々の `Vec` に持つ 4 値 (層 2 用)。
struct Split {
    w: u32,
    a: Vec<u64>,
    b: Vec<u64>,
}

impl Split {
    fn of(x: &LogicVec) -> Split {
        let r = x.as_ref();
        Split {
            w: x.width(),
            a: r.aval().to_vec(),
            b: r.bval().to_vec(),
        }
    }
    fn dirty(w: u32) -> Split {
        Split::of(&xs(w))
    }
    fn r(&self) -> LogicRef<'_> {
        LogicRef::new(self.w, &self.a, &self.b)
    }
    fn m(&mut self) -> LogicMut<'_> {
        LogicMut::new(self.w, &mut self.a, &mut self.b)
    }
    fn vec(&self) -> LogicVec {
        LogicVec::from_ref(self.r())
    }
}

fn bin_l3(op: Bin, x: &LogicVec, y: &LogicVec) -> LogicVec {
    match op {
        Bin::Add => x.add(y),
        Bin::Sub => x.sub(y),
        Bin::Mul => x.mul(y),
        Bin::Div(s) => x.div(y, s),
        Bin::Rem(s) => x.rem(y, s),
    }
}

fn bin_l2(op: Bin, x: &LogicVec, y: &LogicVec) -> LogicVec {
    let (xs_, ys) = (Split::of(x), Split::of(y));
    let mut d = Split::dirty(x.width());
    let mut s = Scratch::for_width(x.width());
    match op {
        Bin::Add => arith::add(d.m(), xs_.r(), ys.r()),
        Bin::Sub => arith::sub(d.m(), xs_.r(), ys.r()),
        Bin::Mul => arith::mul(d.m(), xs_.r(), ys.r(), &mut s),
        Bin::Div(sg) => arith::div(d.m(), xs_.r(), ys.r(), sg, &mut s),
        Bin::Rem(sg) => arith::rem(d.m(), xs_.r(), ys.r(), sg, &mut s),
    }
    d.vec()
}

fn bin_l1(op: Bin, x: L64, y: L64, w: u32) -> L64 {
    match op {
        Bin::Add => arith::add_l64(x, y, w),
        Bin::Sub => arith::sub_l64(x, y, w),
        Bin::Mul => arith::mul_l64(x, y, w),
        Bin::Div(s) => arith::div_l64(x, y, w, s),
        Bin::Rem(s) => arith::rem_l64(x, y, w, s),
    }
}

fn bin2_l3(op: Bin, x: &BitVec, y: &BitVec) -> BitVec {
    match op {
        Bin::Add => x.add(y),
        Bin::Sub => x.sub(y),
        Bin::Mul => x.mul(y),
        Bin::Div(s) => x.div(y, s),
        Bin::Rem(s) => x.rem(y, s),
    }
}

fn bin2_l2(op: Bin, x: &BitVec, y: &BitVec) -> BitVec {
    let w = x.width();
    let mut d = BitVec::filled(w, true);
    let mut s = Scratch::for_width(w);
    let (xr, yr): (BitRef<'_>, BitRef<'_>) = (x.as_ref(), y.as_ref());
    let dm: BitMut<'_> = d.as_mut();
    match op {
        Bin::Add => arith::add2(dm, xr, yr),
        Bin::Sub => arith::sub2(dm, xr, yr),
        Bin::Mul => arith::mul2(dm, xr, yr, &mut s),
        Bin::Div(sg) => arith::div2(dm, xr, yr, sg, &mut s),
        Bin::Rem(sg) => arith::rem2(dm, xr, yr, sg, &mut s),
    }
    d
}

fn bin2_l1(op: Bin, x: u64, y: u64, w: u32) -> u64 {
    match op {
        Bin::Add => arith::add2_l64(x, y, w),
        Bin::Sub => arith::sub2_l64(x, y, w),
        Bin::Mul => arith::mul2_l64(x, y, w),
        Bin::Div(s) => arith::div2_l64(x, y, w, s),
        Bin::Rem(s) => arith::rem2_l64(x, y, w, s),
    }
}

/// 2 項演算の 1 行を全層で確かめる。`want2` は 2 値版の期待値 (`None` なら 4 値の期待値を 2 値化したもの)。
fn check_bin(op: Bin, x: &LogicVec, y: &LogicVec, want: &LogicVec, want2: Option<&LogicVec>) {
    let w = x.width();
    let ctx = format!("{op:?} w={w} {x:?} {y:?}");
    assert_eq!(&bin_l3(op, x, y), want, "{ctx} L3");
    assert_eq!(&bin_l2(op, x, y), want, "{ctx} L2");
    if w <= 64 {
        let got = bin_l1(op, L64::load(x.as_ref()), L64::load(y.as_ref()), w);
        assert_eq!(got, L64::load(want.as_ref()), "{ctx} L1");
    }
    if !x.has_unknown() && !y.has_unknown() {
        let (x2, y2) = (x.to_bit_vec(), y.to_bit_vec());
        let want2 = want2.unwrap_or(want).to_bit_vec();
        assert_eq!(bin2_l3(op, &x2, &y2), want2, "{ctx} 2-state L3");
        assert_eq!(bin2_l2(op, &x2, &y2), want2, "{ctx} 2-state L2");
        if w <= 64 {
            let got = bin2_l1(op, x2.to_u64().unwrap(), y2.to_u64().unwrap(), w);
            assert_eq!(got, want2.to_u64().unwrap(), "{ctx} 2-state L1");
        }
    }
}

fn check_un(op: Un, x: &LogicVec, want: &LogicVec) {
    let w = x.width();
    let ctx = format!("{op:?} w={w} {x:?}");
    let l3 = match op {
        Un::Neg => x.neg(),
        Un::Inc => x.inc(),
        Un::Dec => x.dec(),
    };
    assert_eq!(&l3, want, "{ctx} L3");
    let mut v = x.clone();
    match op {
        Un::Neg => v.neg_assign(),
        Un::Inc => v.inc_assign(),
        Un::Dec => v.dec_assign(),
    }
    assert_eq!(&v, want, "{ctx} L3 assign");
    let xs_ = Split::of(x);
    let mut d = Split::dirty(w);
    match op {
        Un::Neg => arith::neg(d.m(), xs_.r()),
        Un::Inc => arith::inc(d.m(), xs_.r()),
        Un::Dec => arith::dec(d.m(), xs_.r()),
    }
    assert_eq!(&d.vec(), want, "{ctx} L2");
    if w <= 64 {
        let l = L64::load(x.as_ref());
        let got = match op {
            Un::Neg => arith::neg_l64(l, w),
            Un::Inc => arith::inc_l64(l, w),
            Un::Dec => arith::dec_l64(l, w),
        };
        assert_eq!(got, L64::load(want.as_ref()), "{ctx} L1");
    }
    if !x.has_unknown() {
        let x2 = x.to_bit_vec();
        let got = match op {
            Un::Neg => x2.neg(),
            Un::Inc => x2.inc(),
            Un::Dec => x2.dec(),
        };
        assert_eq!(got, want.to_bit_vec(), "{ctx} 2-state");
    }
}

// ===========================================================================
// TC24 加減算・符号反転・増減 (§11.4.3, §11.4.2)
// ===========================================================================

/// §11.4.3: 結果は幅で切り詰め、桁あふれは捨てる。ワード境界の桁上げ・借りも確かめる。
#[test]
fn add_sub_11_4_3() {
    check_bin(Bin::Add, &hx(8, "ff"), &u(8, 1), &u(8, 0), None);
    check_bin(Bin::Add, &ones(64), &u(64, 1), &u(64, 0), None);
    check_bin(
        Bin::Add,
        &ones(64).resize(65, false),
        &u(65, 1),
        &bit(65, 64),
        None,
    );
    check_bin(Bin::Add, &ones(128), &u(128, 1), &u(128, 0), None);
    check_bin(
        Bin::Add,
        &ones(128).resize(129, false),
        &u(129, 1),
        &bit(129, 128),
        None,
    );
    check_bin(Bin::Sub, &u(8, 0), &u(8, 1), &hx(8, "ff"), None);
    check_bin(
        Bin::Sub,
        &bit(65, 64),
        &u(65, 1),
        &ones(64).resize(65, false),
        None,
    );
}

/// §11.4.3: オペランドに X/Z が 1 ビットでもあれば全 X。
#[test]
fn add_unknown_11_4_3() {
    check_bin(Bin::Add, &lv("000x"), &lv("0001"), &xs(4), None);
    check_bin(Bin::Add, &lv("0z00"), &lv("0000"), &xs(4), None);
    let mut x = LogicVec::zero(129);
    x.set_bit(128, Logic::X);
    check_bin(Bin::Add, &x, &u(129, 1), &xs(129), None);
    check_bin(Bin::Sub, &u(129, 1), &x, &xs(129), None);
}

/// §11.4.3 の単項 `-` と §11.4.2 の `++ --`。
#[test]
fn neg_inc_dec_11_4_2() {
    check_un(Un::Neg, &u(8, 1), &hx(8, "ff"));
    check_un(Un::Neg, &hx(8, "80"), &hx(8, "80"));
    check_un(Un::Neg, &u(8, 0), &u(8, 0));
    check_un(Un::Neg, &u(1, 1), &u(1, 1));
    check_un(Un::Neg, &u(130, 1), &ones(130));
    check_un(Un::Inc, &hx(8, "ff"), &u(8, 0));
    check_un(Un::Dec, &u(8, 0), &hx(8, "ff"));
    check_un(Un::Inc, &ones(64).resize(128, false), &bit(128, 64));
    check_un(Un::Dec, &bit(128, 64), &ones(64).resize(128, false));
    check_un(Un::Inc, &lv("1x00"), &xs(4));
    check_un(Un::Neg, &lv("z"), &xs(1));
}

// ===========================================================================
// TC25 乗算 (§11.4.3)
// ===========================================================================

/// §11.4.3: 下位 `w` ビットだけを計算する (符号付きでも符号なしでも同じ)。
#[test]
fn mul_11_4_3() {
    check_bin(Bin::Mul, &hx(8, "10"), &hx(8, "10"), &u(8, 0), None);
    check_bin(Bin::Mul, &hx(8, "ff"), &hx(8, "ff"), &u(8, 1), None);
    check_bin(Bin::Mul, &ones(64), &ones(64), &u(64, 1), None);
    check_bin(Bin::Mul, &ones(65), &ones(65), &u(65, 1), None);
    check_bin(
        Bin::Mul,
        &ones(64).resize(128, false),
        &ones(64).resize(128, false),
        &hx(128, "ffff_ffff_ffff_fffe_0000_0000_0000_0001"),
        None,
    );
    check_bin(Bin::Mul, &ones(128), &ones(128), &u(128, 1), None);
    check_bin(Bin::Mul, &lv("0011"), &lv("000z"), &xs(4), None);
    check_bin(Bin::Mul, &u(1, 1), &u(1, 1), &u(1, 1), None);
    check_bin(Bin::Mul, &u(1, 1), &u(1, 0), &u(1, 0), None);
}

// ===========================================================================
// TC26 除算と剰余 (§11.4.3)
// ===========================================================================

/// 除算と剰余の 1 行: `(x, y, signed, x / y, x % y)`。期待値の `None` は全 X (2 値版は 0、V-2)。
fn check_divrem(
    x: &LogicVec,
    y: &LogicVec,
    signed: bool,
    q: Option<&LogicVec>,
    r: Option<&LogicVec>,
) {
    let w = x.width();
    let zero = LogicVec::zero(w);
    let (qx, rx) = (xs(w), xs(w));
    let q4 = q.unwrap_or(&qx);
    let r4 = r.unwrap_or(&rx);
    check_bin(Bin::Div(signed), x, y, q4, Some(q.unwrap_or(&zero)));
    check_bin(Bin::Rem(signed), x, y, r4, Some(r.unwrap_or(&zero)));
}

/// §11.4.3: 0 方向へ切り捨て、剰余の符号は被除数の符号。
#[test]
fn div_rem_truncation_11_4_3() {
    check_divrem(&u(8, 200), &u(8, 7), false, Some(&u(8, 28)), Some(&u(8, 4)));
    check_divrem(&i(8, -7), &u(8, 2), true, Some(&i(8, -3)), Some(&i(8, -1)));
    check_divrem(&u(8, 7), &i(8, -2), true, Some(&i(8, -3)), Some(&u(8, 1)));
    check_divrem(&i(8, -7), &i(8, -2), true, Some(&u(8, 3)), Some(&i(8, -1)));
    check_divrem(
        &hx(8, "f9"),
        &u(8, 2),
        false,
        Some(&hx(8, "7c")),
        Some(&u(8, 1)),
    );
    check_divrem(
        &hx(8, "80"),
        &hx(8, "ff"),
        false,
        Some(&u(8, 0)),
        Some(&u(8, 128)),
    );
    check_divrem(&u(1, 1), &u(1, 1), false, Some(&u(1, 1)), Some(&u(1, 0)));
}

/// §11.4.3: 符号付きの最小値 / -1 は 2 の補数で折り返して最小値、剰余 0 [要VCS確認 V-1]。
#[test]
fn div_min_by_minus_one_v1() {
    check_divrem(
        &hx(8, "80"),
        &hx(8, "ff"),
        true,
        Some(&hx(8, "80")),
        Some(&u(8, 0)),
    );
    check_divrem(
        &bit(64, 63),
        &ones(64),
        true,
        Some(&bit(64, 63)),
        Some(&u(64, 0)),
    );
    check_divrem(
        &bit(65, 64),
        &ones(65),
        true,
        Some(&bit(65, 64)),
        Some(&u(65, 0)),
    );
    check_divrem(
        &bit(1024, 1023),
        &ones(1024),
        true,
        Some(&bit(1024, 1023)),
        Some(&u(1024, 0)),
    );
    // 幅 1 の符号付き: '1' は -1。(-1) / (-1) = 1 → 1 ビットでは '1'。
    check_divrem(&u(1, 1), &u(1, 1), true, Some(&u(1, 1)), Some(&u(1, 0)));
}

/// §11.4.3: 除数が 0 なら全 X (4 値)。2 値版は 0 [要VCS確認 V-2]。
#[test]
fn div_by_zero_11_4_3_v2() {
    for signed in [false, true] {
        check_divrem(&u(8, 5), &u(8, 0), signed, None, None);
        check_divrem(&u(8, 0), &u(8, 0), signed, None, None);
        check_divrem(&u(1, 1), &u(1, 0), signed, None, None);
        check_divrem(&ones(200), &u(200, 0), signed, None, None);
    }
    // 未知のオペランドも全 X。
    check_bin(Bin::Div(false), &lv("0000_x101"), &u(8, 1), &xs(8), None);
    check_bin(Bin::Rem(false), &lv("0000_x101"), &u(8, 1), &xs(8), None);
}

/// 多ワードの除算: 除数 1 ワード / 多ワード、被除数 < 除数、Knuth D の add back (test-plan TC26)。
#[test]
fn div_multiword_11_4_3() {
    // (2^128 - 1) / (2^64 + 1) = 2^64 - 1、剰余 0
    let v = u(128, 1).add(&bit(128, 64));
    check_divrem(
        &ones(128),
        &v,
        false,
        Some(&ones(64).resize(128, false)),
        Some(&u(128, 0)),
    );
    // 2^128 / 3 (幅 129、除数 1 ワード)
    check_divrem(
        &bit(129, 128),
        &u(129, 3),
        false,
        Some(&hx(129, "5555_5555_5555_5555_5555_5555_5555_5555")),
        Some(&u(129, 1)),
    );
    // Knuth D の補正 "add back" が起きる入力
    let uu = hx(
        256,
        "7fff_ffff_ffff_ffff_8000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000",
    );
    let vv = hx(
        256,
        "8000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0001",
    );
    check_divrem(
        &uu,
        &vv,
        false,
        Some(&hx(256, "ffff_ffff_ffff_fffe")),
        Some(&hx(
            256,
            "7fff_ffff_ffff_ffff_ffff_ffff_ffff_ffff_0000_0000_0000_0002",
        )),
    );
    // 被除数 < 除数: 商 0、剰余 = 被除数
    check_divrem(&vv, &uu, false, Some(&u(256, 0)), Some(&vv));
    // 符号付きの多ワード: -(2^100 + 7) / 2^64 = -(2^36)、剰余 -7
    let big = bit(130, 100).add(&u(130, 7));
    check_divrem(
        &big.neg(),
        &bit(130, 64),
        true,
        Some(&bit(130, 36).neg()),
        Some(&i(130, -7)),
    );
    check_divrem(
        &big,
        &bit(130, 64).neg(),
        true,
        Some(&bit(130, 36).neg()),
        Some(&u(130, 7)),
    );
}

// ===========================================================================
// TC27 べき乗 (§11.4.3 のべき乗の規則)
// ===========================================================================

/// べき乗の 1 行を全層で確かめる。`want` の `None` は全 X (2 値版は 0、V-2)。
fn check_pow(base: &LogicVec, bs: bool, exp: &LogicVec, es: bool, want: Option<&LogicVec>) {
    let w = base.width();
    let ctx = format!("pow w={w} {base:?} (S={bs}) ** {exp:?} (S={es})");
    let all_x = xs(w);
    let want4 = want.unwrap_or(&all_x);
    // 層 3
    assert_eq!(&base.pow(bs, exp, es), want4, "{ctx} L3");
    // 層 2
    let (b2, e2) = (Split::of(base), Split::of(exp));
    let mut d = Split::dirty(w);
    let mut s = Scratch::for_width(w);
    arith::pow(d.m(), b2.r(), bs, e2.r(), es, &mut s);
    assert_eq!(&d.vec(), want4, "{ctx} L2");
    // 層 1
    if w <= 64 && exp.width() <= 64 {
        let got = arith::pow_l64(
            L64::load(base.as_ref()),
            w,
            bs,
            L64::load(exp.as_ref()),
            exp.width(),
            es,
        );
        assert_eq!(got, L64::load(want4.as_ref()), "{ctx} L1");
    }
    if base.has_unknown() {
        return;
    }
    // 混在 (2 値 base × 4 値の指数、§3.5.1): 4 値の結果は同じ。
    let bb = base.to_bit_vec();
    assert_eq!(&bb.pow_logic(bs, exp, es), want4, "{ctx} pow_logic");
    let mut d = Split::dirty(w);
    arith::pow_bits(d.m(), bb.as_ref(), bs, e2.r(), es, &mut s);
    assert_eq!(&d.vec(), want4, "{ctx} pow_bits");
    if w <= 64 && exp.width() <= 64 {
        let got = arith::pow_bits_l64(
            bb.to_u64().unwrap(),
            w,
            bs,
            L64::load(exp.as_ref()),
            exp.width(),
            es,
        );
        assert_eq!(got, L64::load(want4.as_ref()), "{ctx} pow_bits_l64");
    }
    if exp.has_unknown() {
        return;
    }
    // 2 値版: X は 0 [V-2]。
    let eb = exp.to_bit_vec();
    let zero = LogicVec::zero(w);
    let want2 = want.unwrap_or(&zero).to_bit_vec();
    assert_eq!(bb.pow(bs, &eb, es), want2, "{ctx} 2-state L3");
    let mut d2 = BitVec::filled(w, true);
    arith::pow2(d2.as_mut(), bb.as_ref(), bs, eb.as_ref(), es, &mut s);
    assert_eq!(d2, want2, "{ctx} 2-state L2");
    if w <= 64 && exp.width() <= 64 {
        let got = arith::pow2_l64(
            bb.to_u64().unwrap(),
            w,
            bs,
            eb.to_u64().unwrap(),
            exp.width(),
            es,
        );
        assert_eq!(got, want2.to_u64().unwrap(), "{ctx} 2-state L1");
    }
}

/// §11.4.3 のべき乗の規則: 指数 0、指数が負の場合の base = 0 / 1 / -1 / |base| > 1。
#[test]
fn pow_special_cases_11_4_3() {
    let s = true;
    // 指数の幅は自己決定なので、いくつかの幅で同じ値を試す。
    for ew in [4u32, 8, 65, 71] {
        let e = |v: i64| i(ew, v);
        check_pow(&u(8, 0), s, &e(0), s, Some(&u(8, 1)));
        check_pow(&u(8, 0), s, &e(-1), s, None); // 0 ** 負 = X (2 値: 0 [V-2])
        check_pow(&u(8, 0), s, &e(3), s, Some(&u(8, 0)));
        check_pow(&u(8, 1), s, &e(-5), s, Some(&u(8, 1)));
        check_pow(&i(8, -1), s, &e(-1), s, Some(&i(8, -1)));
        check_pow(&i(8, -1), s, &e(-2), s, Some(&u(8, 1)));
        check_pow(&i(8, -1), s, &e(3), s, Some(&i(8, -1)));
        check_pow(&u(8, 2), s, &e(-1), s, Some(&u(8, 0)));
        check_pow(&i(8, -2), s, &e(-3), s, Some(&u(8, 0)));
    }
    // 多ワードの base でも同じ規則。
    check_pow(&u(200, 0), s, &i(8, -1), s, None);
    check_pow(&u(200, 1), s, &i(8, -3), s, Some(&u(200, 1)));
    check_pow(&ones(200), s, &i(8, -3), s, Some(&ones(200)));
    check_pow(&ones(200), s, &i(8, -4), s, Some(&u(200, 1)));
    check_pow(&ones(200), false, &i(8, -4), s, Some(&u(200, 0)));
    check_pow(&u(200, 0), s, &u(8, 0), s, Some(&u(200, 1)));
}

/// §11.4.3: 符号なしの base / 指数と、2^w の剰余環での計算。
#[test]
fn pow_unsigned_11_4_3() {
    let us = false;
    check_pow(&u(8, 2), us, &u(8, 0), us, Some(&u(8, 1)));
    check_pow(&u(8, 2), us, &u(8, 7), us, Some(&hx(8, "80")));
    check_pow(&u(8, 2), us, &u(8, 8), us, Some(&u(8, 0)));
    check_pow(&u(8, 3), us, &u(8, 5), us, Some(&u(8, 243)));
    check_pow(&u(8, 3), us, &u(8, 6), us, Some(&u(8, 217)));
    check_pow(&u(8, 3), us, &hx(8, "ff"), us, Some(&u(8, 171)));
    // base が符号なしなら 8'hff は 255 で、-1 扱いしない (|base| > 1 → 0)。
    check_pow(&hx(8, "ff"), us, &hx(8, "ff"), true, Some(&u(8, 0)));
    // 指数が符号なしなら 8'hff は 255 (奇数) で、(-1)^255 = -1。
    check_pow(&i(8, -1), true, &hx(8, "ff"), us, Some(&i(8, -1)));
    // 巨大な指数
    check_pow(&u(8, 3), us, &bit(65, 64), us, Some(&u(8, 1)));
    check_pow(&u(8, 2), us, &bit(71, 70), us, Some(&u(8, 0)));
    check_pow(&u(8, 1), us, &bit(71, 70), us, Some(&u(8, 1)));
    // 幅 128: 3^100 mod 2^128
    check_pow(
        &u(128, 3),
        us,
        &u(8, 100),
        us,
        Some(&hx(128, "6737_6856_5b41_f775_d694_7d55_cf38_13d1")),
    );
    check_pow(&u(1, 1), us, &u(4, 0), us, Some(&u(1, 1)));
    // 幅 1 の符号付き base '1' は -1: (-1)^(-1) = -1、(-1)^(-2) = 1。どちらも 1 ビットでは '1'。
    check_pow(&u(1, 1), true, &lv("11"), true, Some(&u(1, 1)));
    check_pow(&u(1, 1), true, &lv("10"), true, Some(&u(1, 1)));
    check_pow(&u(1, 0), true, &lv("1"), true, None);
}

/// §11.4.3: base か指数に未知があれば全 X (指数が 0 でも)。
#[test]
fn pow_unknown_11_4_3() {
    check_pow(&lv("0000_00x1"), false, &u(4, 2), false, None);
    check_pow(&u(8, 3), false, &lv("00z1"), false, None);
    check_pow(&lv("0000_000x"), false, &u(4, 0), false, None);
    check_pow(&u(130, 3), false, &lv("x"), false, None);
}

// ===========================================================================
// TC34 混在カーネル pow_bits (plan.md §3.5.1、決定 R-2)
// ===========================================================================

/// §3.5.1: 2 値の base × 4 値の指数は 4 値で評価する [要VCS確認 V-9]。
#[test]
fn pow_bits_mixed_3_5_1() {
    let mut s = Scratch::for_width(8);
    let run = |base: u64, bs: bool, exp: &str, es: bool, s: &mut Scratch| {
        let b = BitVec::from_u64(8, base);
        let e = lv(exp);
        let mut d = LogicVec::filled(8, Logic::Z);
        arith::pow_bits(d.as_mut(), b.as_ref(), bs, e.as_ref(), es, s);
        assert_eq!(b.pow_logic(bs, &e, es), d, "pow_logic と pow_bits");
        d
    };
    assert_eq!(run(3, false, "0010", false, &mut s), u(8, 9));
    assert_eq!(run(3, false, "00x0", false, &mut s), xs(8));
    // 4 値の結果なので x (2 値の変数への代入で 0 にするのは呼び出し側)。
    assert_eq!(run(0, true, "1111", true, &mut s), xs(8));
}
