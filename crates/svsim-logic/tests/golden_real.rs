//! 実数との変換のゴールデンテスト (IEEE 1800-2023 §6.12.1, §20.5、plan.md §3.5(i)、
//! test-plan TC42, TC43)。
//!
//! 期待値は test-plan の表をそのまま写したもの。`RealToIntStatus` の期待値も TC43 の表 (Q-6、
//! STATUS の決定「from_f64_trunc で小数部を捨てたら Rounded」) による。

use svsim_logic::real::{self, RealToIntStatus};
use svsim_logic::{BitVec, Logic, LogicVec};

use RealToIntStatus::{Exact, NotFinite, Rounded, Wrapped};

fn u(w: u32, v: u64) -> LogicVec {
    LogicVec::from_u64(w, v)
}

/// 幅 `w` で、ビット `ks` だけ 1。
fn bits(w: u32, ks: &[u32]) -> LogicVec {
    let mut x = LogicVec::zero(w);
    for &k in ks {
        x.set_bit(k, Logic::L1);
    }
    x
}

/// 幅 `w` で、ビット `lo..hi` が 1。
fn ones(w: u32, lo: u32, hi: u32) -> LogicVec {
    let mut x = LogicVec::zero(w);
    for k in lo..hi {
        x.set_bit(k, Logic::L1);
    }
    x
}

fn lv(s: &str) -> LogicVec {
    LogicVec::from_bits_str(&s.replace('_', "")).unwrap()
}

fn p2(k: i32) -> f64 {
    2f64.powi(k)
}

/// TC42: 整数 → 実数 (§6.12.1, §20.5)。最近接偶数で 1 回だけ丸める (R-12)、溢れは ±∞ (Q-5)。
#[test]
fn tc42_int_to_real_6_12_1() {
    assert_eq!(real::to_f64(u(32, 0xffff_ffff).as_ref(), true), -1.0);
    assert_eq!(
        real::to_f64(u(32, 0xffff_ffff).as_ref(), false),
        4294967295.0
    );
    assert_eq!(u(64, u64::MAX).to_f64(false), 18446744073709551616.0);
    assert_eq!(u(64, (1 << 53) + 1).to_f64(false), p2(53)); // 同距離 → 偶数
    assert_eq!(u(64, (1 << 53) + 3).to_f64(false), p2(53) + 4.0);
    assert_eq!(bits(128, &[100]).to_f64(false), p2(100));
    let all1024 = LogicVec::filled(1024, Logic::L1);
    assert_eq!(all1024.to_f64(false), f64::INFINITY); // Q-5
    assert_eq!(bits(1025, &[1024]).to_f64(true), f64::NEG_INFINITY); // Q-5
    // 2^1024 - 2^971 = f64::MAX (正確)。
    assert_eq!(ones(1024, 971, 1024).to_f64(false), f64::MAX);
    // 2^1024 - 2^970: f64::MAX と 2^1024 の中間 → 偶数側の 2^1024 → 溢れ (Q-5)。
    assert_eq!(ones(1024, 970, 1024).to_f64(false), f64::INFINITY);
    // 2^1024 - 2^970 - 1 → f64::MAX。
    let mut below = ones(1024, 970, 1024);
    below.set_bit(970, Logic::L0);
    for k in 0..970 {
        below.set_bit(k, Logic::L1);
    }
    assert_eq!(below.to_f64(false), f64::MAX);
    assert_eq!(all1024.to_f64(true), -1.0);
    // [要VCS確認 V-14] x/z は 0 として扱う。
    assert_eq!(lv("0000_1x01").to_f64(false), 9.0);
    assert_eq!(lv("1x01").to_f64(true), -7.0);
    assert_eq!(lv("zzzz_zzzz").to_f64(false), 0.0);

    assert_eq!(u(64, (1 << 24) + 1).to_f32(false), 16777216.0);
    // 二重丸めの検出 (R-12): f64 を経由すると 2^54 になる。
    let v = (1u64 << 54) + (1 << 30) + 1;
    assert_eq!(real::to_f32(u(64, v).as_ref(), false), 18014400656965632.0);
    assert_eq!(18014400656965632.0f32, (p2(54) + p2(31)) as f32);
    assert_ne!((v as f64) as f32, 18014400656965632.0); // 経由した場合との違い
    assert_eq!(u(64, 1 << 63).to_f32(true), -(2f32.powi(63)));
    // 2^128 - 2^104 = f32::MAX。
    assert_eq!(ones(128, 104, 128).to_f32(false), f32::MAX);
    // 2^128 - 2^104 + 2^103 - 1 → f32::MAX。
    let mut x = ones(128, 104, 128);
    for k in 0..103 {
        x.set_bit(k, Logic::L1);
    }
    assert_eq!(x.to_f32(false), f32::MAX);
    // 2^128 - 2^104 + 2^103 → 中間 → 偶数側の 2^128 → +∞ (Q-5)。
    let mut x = ones(128, 104, 128);
    x.set_bit(103, Logic::L1);
    assert_eq!(x.to_f32(false), f32::INFINITY);
    assert_eq!(bits(129, &[128]).to_f32(false), f32::INFINITY);

    // 2 値版も同じ。
    let b = BitVec::from_u64(32, 0xffff_ffff);
    assert_eq!(real::to_f64_2(b.as_ref(), true), -1.0);
    assert_eq!(real::to_f32_2(b.as_ref(), false), 4294967296.0);
}

fn round(w: u32, v: f64) -> (LogicVec, RealToIntStatus) {
    let r = LogicVec::from_f64_round(w, v);
    // 2 値版も同じ値と状態になる。
    let b = BitVec::from_f64_round(w, v);
    assert_eq!(LogicVec::from(&b.0), r.0, "from_f64_round2 w={w} v={v}");
    assert_eq!(b.1, r.1);
    r
}

fn trunc(w: u32, v: f64) -> (LogicVec, RealToIntStatus) {
    let r = LogicVec::from_f64_trunc(w, v);
    let b = BitVec::from_f64_trunc(w, v);
    assert_eq!(LogicVec::from(&b.0), r.0, "from_f64_trunc2 w={w} v={v}");
    assert_eq!(b.1, r.1);
    r
}

fn i(w: u32, v: i64) -> LogicVec {
    LogicVec::from_i64(w, v)
}

/// TC43: 実数 → 整数 (§6.12.1: 半分は 0 から遠い方へ、§20.5 `$rtoi`) と状態 (Q-6)。
#[test]
fn tc43_real_to_int_6_12_1() {
    assert_eq!(round(32, 35.5), (i(32, 36), Rounded));
    assert_eq!(round(32, 35.4), (i(32, 35), Rounded));
    assert_eq!(round(32, 36.0), (i(32, 36), Exact));
    // 0 から遠い方へ (偶数丸めではない)。
    assert_eq!(round(32, -1.5), (i(32, -2), Rounded));
    assert_eq!(round(32, -2.5), (i(32, -3), Rounded));
    assert_eq!(round(32, 2.5), (i(32, 3), Rounded));
    assert_eq!(round(32, 0.5), (i(32, 1), Rounded));
    assert_eq!(round(32, -0.5), (i(32, -1), Rounded));
    // floor(x + 0.5) だと 1 になる落とし穴。
    assert_eq!(round(32, 0.49999999999999994), (i(32, 0), Rounded));
    assert_eq!(round(32, 1.4999999999999998), (i(32, 1), Rounded));
    assert_eq!(round(32, -0.0), (i(32, 0), Exact));
    assert_eq!(round(32, 5e-324), (i(32, 0), Rounded));
    assert_eq!(
        round(64, 4503599627370495.5),
        (i(64, 4503599627370496), Rounded)
    );
    assert_eq!(round(8, -1.0), (u(8, 0xff), Exact));
    assert_eq!(round(8, 255.0), (u(8, 0xff), Exact));
    assert_eq!(round(8, -128.0), (u(8, 0x80), Exact));
    assert_eq!(round(8, 254.6), (u(8, 0xff), Rounded));
    assert_eq!(round(8, 255.5), (u(8, 0x00), Wrapped));
    assert_eq!(round(8, 300.0), (u(8, 44), Wrapped));
    assert_eq!(round(8, -129.0), (u(8, 0x7f), Wrapped));
    assert_eq!(round(64, 1e20), (u(64, 7766279631452241920), Wrapped));
    // 1e20 = 0x5_6BC7_5E2D_6310_0000 (128 ビットで正確)。
    let mut e20 = LogicVec::zero(128);
    let v: u128 = 100_000_000_000_000_000_000;
    for k in 0..128 {
        if (v >> k) & 1 != 0 {
            e20.set_bit(k, Logic::L1);
        }
    }
    assert_eq!(round(128, 1e20), (e20, Exact));
    assert_eq!(round(128, p2(100)), (bits(128, &[100]), Exact));
    // -2^100 = 2^128 - 2^100。
    assert_eq!(round(128, -p2(100)), (ones(128, 100, 128), Exact));
    // 1e300 の正確な整数値 (997 ビット)。to_f64 で戻すと 1e300 (正確に表せる値なので)。
    let (x, st) = round(1100, 1e300);
    assert_eq!(st, Exact);
    assert_eq!(x.to_f64(true), 1e300);
    let top = (0..1100).rev().find(|&k| x.bit(k) == Logic::L1).unwrap();
    assert_eq!(top + 1, 997);
    // [要VCS確認 V-4] NaN / ±∞ は 0。
    for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(round(32, v), (u(32, 0), NotFinite));
    }

    assert_eq!(trunc(32, 35.9), (i(32, 35), Rounded));
    assert_eq!(trunc(32, -35.9), (i(32, -35), Rounded));
    assert_eq!(trunc(32, -0.5), (i(32, 0), Rounded));
    assert_eq!(trunc(32, 7.0), (i(32, 7), Exact));
    assert_eq!(trunc(8, 256.7), (u(8, 0x00), Wrapped));
    assert_eq!(trunc(32, f64::NAN), (u(32, 0), NotFinite)); // [要VCS確認 V-4]
}

/// TC43: 出力先に入っていた値 (X/Z を含む) は全部上書きされ、bval は 0 になる。
#[test]
fn tc43_overwrites_dst() {
    let mut d = LogicVec::filled(100, Logic::X);
    assert_eq!(real::from_f64_round(d.as_mut(), 3.0), Exact);
    assert_eq!(d, u(100, 3));
    let mut d = LogicVec::filled(100, Logic::Z);
    assert_eq!(real::from_f64_trunc(d.as_mut(), f64::NAN), NotFinite);
    assert_eq!(d, u(100, 0));
}

/// TC43: `$realtobits` / `$bitstoreal` / `$shortrealtobits` / `$bitstoshortreal` (§20.5)。
#[test]
fn tc43_bits_20_5() {
    assert_eq!(real::real_to_bits(1.0), 0x3ff0_0000_0000_0000);
    assert_eq!(real::bits_to_real(0x3ff0_0000_0000_0000), 1.0);
    assert_eq!(real::shortreal_to_bits(1.0), 0x3f80_0000);
    assert_eq!(real::bits_to_shortreal(0x3f80_0000), 1.0);
    assert_eq!(real::real_to_bits(-0.0), 0x8000_0000_0000_0000);
    // NaN のビットパターンをそのまま保つ。
    for nan in [0x7ff8_0000_0000_0001u64, 0xfff4_0000_dead_beef] {
        assert_eq!(real::real_to_bits(real::bits_to_real(nan)), nan);
    }
    let nan32 = 0x7fc0_1234u32;
    assert_eq!(
        real::shortreal_to_bits(real::bits_to_shortreal(nan32)),
        nan32
    );
}
