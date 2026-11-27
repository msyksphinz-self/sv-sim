//! 構造演算のゴールデンテスト (規格の例と境界を固定値で書く。plan.md §5.4、test-plan TC29, TC31〜TC34,
//! TC36, TC53)。
//!
//! テスト名とコメントには IEEE 1800-2023 の条番号を書く。`[要VCS確認 V-n]` の印は、暫定の挙動
//! (plan.md §11) を固定しているテストに付ける。

use svsim_logic::ops::select::{self, Index};
use svsim_logic::ops::{concat, math, resize, shift};
use svsim_logic::{BitVec, L64, Logic, LogicVec};

fn lv(s: &str) -> LogicVec {
    LogicVec::from_bits_str(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn bv(s: &str) -> BitVec {
    BitVec::from_bits_str(s).unwrap_or_else(|e| panic!("{s:?}: {e}"))
}

fn bits(v: &LogicVec) -> String {
    assert!(v.as_ref().is_normalized(), "not normalized: {v:?}");
    v.to_bits_string()
}

fn bits2(v: &BitVec) -> String {
    assert!(v.as_ref().is_normalized(), "not normalized: {v:?}");
    v.to_bits_string()
}

fn hex8(v: u64) -> LogicVec {
    LogicVec::from_u64(8, v)
}

/// 幅 `w` で、ビット `set` だけが 1 の値。
fn one_hot(w: u32, set: &[u32]) -> LogicVec {
    let mut v = LogicVec::zero(w);
    for &i in set {
        v.set_bit(i, Logic::L1);
    }
    v
}

// ===========================================================================
// TC29 シフト (§11.4.10)
// ===========================================================================

/// §11.4.10: 論理シフトは空いたビットを 0 で埋め、`>>>` は符号付きなら符号ビットで埋める。
/// データの x/z はそのまま移動する。
#[test]
fn shift_11_4_10_data_xz_moves() {
    let x = lv("1001x0z1");
    let two = LogicVec::from_u64(3, 2);
    assert_eq!(bits(&x.shl(&two)), "01x0z100");
    assert_eq!(bits(&x.shr(&two)), "001001x0");
    assert_eq!(bits(&x.ashr(&two, true)), "111001x0");
    assert_eq!(bits(&x.ashr(&two, false)), "001001x0");
    // シフト量 0 は入力と同じ (x/z も含め)
    assert_eq!(x.shl(&LogicVec::zero(4)), x);
    assert_eq!(x.ashr_const(0, true), x);
}

/// §11.4.10: シフト量が幅以上なら `<<` `>>` は 0、`>>>` (符号付き) は全ビットが符号ビットの値。
#[test]
fn shift_11_4_10_amount_ge_width() {
    let one = hex8(1);
    for n in [8u64, 9] {
        assert_eq!(one.shl_const(n), LogicVec::zero(8));
        assert_eq!(one.shl(&LogicVec::from_u64(8, n)), LogicVec::zero(8));
    }
    // 2^70
    let mut big = LogicVec::zero(71);
    big.set_bit(70, Logic::L1);
    assert_eq!(one.shl(&big), LogicVec::zero(8));
    assert_eq!(one.shl_const(u64::MAX), LogicVec::zero(8));
    let hundred = LogicVec::from_u64(7, 100);
    assert_eq!(hex8(0x80).ashr(&hundred, true), hex8(0xff));
    assert_eq!(hex8(0x7f).ashr(&hundred, true), hex8(0x00));
    // シフト量 = 128 ビットで 2^64 (上位ワードに 1): 切り詰めない
    let mut a = LogicVec::zero(128);
    a.set_bit(64, Logic::L1);
    assert_eq!(one.shl(&a), LogicVec::zero(8));
    assert_eq!(hex8(0x80).shr(&a), LogicVec::zero(8));
}

/// §11.4.10: `>>>` の符号ビットが x なら x、z なら z で埋める [要VCS確認 V-3]。
#[test]
fn shift_11_4_10_ashr_sign_xz_v3() {
    let one = LogicVec::from_u64(2, 1);
    assert_eq!(bits(&lv("x001").ashr(&one, true)), "xx00");
    assert_eq!(bits(&lv("z000").ashr(&one, true)), "zz00");
    assert_eq!(
        bits(&lv("z000").ashr(&LogicVec::from_u64(4, 8), true)),
        "zzzz"
    );
    assert_eq!(bits(&lv("z000").ashr_const(8, true)), "zzzz");
}

/// §11.4.10: シフト量に x/z があれば全 x。シフト量は常に符号なし。
#[test]
fn shift_11_4_10_unknown_amount_and_unsigned_amount() {
    let one = hex8(1);
    assert_eq!(bits(&one.shl(&lv("x01"))), "xxxxxxxx");
    assert_eq!(bits(&one.shl(&lv("0z1"))), "xxxxxxxx");
    assert_eq!(bits(&one.ashr(&lv("x"), true)), "xxxxxxxx");
    // 4'b1111 は符号付きの型でも 15 として扱う
    assert_eq!(one.shl(&lv("1111")), LogicVec::zero(8));
    assert_eq!(hex8(0x80).shr(&lv("111")), hex8(0x01));
}

/// §11.4.10: ワード境界をまたぐシフト。
#[test]
fn shift_11_4_10_word_boundaries() {
    let one64 = LogicVec::from_u64(64, 1);
    assert_eq!(one64.shl_const(63), one_hot(64, &[63]));
    assert_eq!(one64.shl_const(64), LogicVec::zero(64));
    assert_eq!(LogicVec::from_u64(65, 1).shl_const(64), one_hot(65, &[64]));
    assert_eq!(one_hot(128, &[63]).shl_const(1), one_hot(128, &[64]));
    let s = one_hot(129, &[128]).ashr_const(64, true);
    let want: Vec<u32> = (64..129).collect();
    assert_eq!(s, one_hot(129, &want));
}

/// §11.4.10: `_const` 版 (n = 0, 7, 8, `u64::MAX`) も同じ規則。層 2・層 1 も確かめる。
#[test]
fn shift_11_4_10_const_versions() {
    let x = lv("1001x0z1");
    let l = L64::load(x.as_ref());
    for n in [0u64, 7, 8, u64::MAX] {
        let amt = LogicVec::from_u64(64, n);
        assert_eq!(x.shl_const(n), x.shl(&amt), "shl {n}");
        assert_eq!(x.shr_const(n), x.shr(&amt), "shr {n}");
        assert_eq!(x.ashr_const(n, true), x.ashr(&amt, true), "ashr {n}");
        let mut d = LogicVec::zero(8);
        shift::shl_const(d.as_mut(), x.as_ref(), n);
        assert_eq!(d, x.shl(&amt));
        assert_eq!(
            shift::ashr_const_l64(l, 8, n, true),
            L64::load(x.ashr(&amt, true).as_ref())
        );
    }
    assert_eq!(bits(&x.shl_const(7)), "10000000");
    assert_eq!(bits(&x.ashr_const(7, true)), "11111111");
}

// ===========================================================================
// TC31 拡張・切り詰め・2 値/4 値の変換 (§11.6.1, §11.8.2, §6.11.2)
// ===========================================================================

/// §11.8.2: 拡張は拡張する前のオペランドの符号で行い、符号ビット (x/z を含む) を複製する。
#[test]
fn resize_11_8_2_sign_extension() {
    assert_eq!(bits(&lv("1010").resize(8, true)), "11111010");
    assert_eq!(bits(&lv("1010").resize(8, false)), "00001010");
    assert_eq!(bits(&lv("x010").resize(8, true)), "xxxxx010");
    assert_eq!(bits(&lv("z010").resize(8, true)), "zzzzz010");
    assert_eq!(bits(&lv("z010").resize(8, false)), "0000z010");
}

/// §11.6.1 / §6.24.1: 切り詰めは下位ビットを残す。
#[test]
fn resize_11_6_1_truncation() {
    for s in [false, true] {
        assert_eq!(bits(&lv("1x0z0110").resize(4, s)), "0110");
        assert_eq!(
            LogicVec::filled(65, Logic::L1).resize(64, s),
            LogicVec::filled(64, Logic::L1)
        );
        let x = lv("1x0z0110");
        assert_eq!(x.resize(8, s), x);
    }
}

/// §11.8.2: ワード境界をまたぐ拡張。
#[test]
fn resize_11_8_2_word_boundaries() {
    let m = LogicVec::from_u64(64, 0x8000_0000_0000_0000);
    assert_eq!(m.resize(65, true), one_hot(65, &[63, 64]));
    assert_eq!(m.resize(65, false), one_hot(65, &[63]));
    assert_eq!(lv("1").resize(129, true), LogicVec::filled(129, Logic::L1));
    assert_eq!(lv("x").resize(129, true), LogicVec::filled(129, Logic::X));
    // 2 値
    assert_eq!(bits2(&bv("1010").resize(8, true)), "11111010");
    assert_eq!(bits2(&bv("1").resize(129, true)), "1".repeat(129));
}

/// §6.11.2: 4 値 → 2 値は x/z → 0、2 値 → 4 値は bval = 0。
#[test]
fn two_state_6_11_2() {
    assert_eq!(bits2(&lv("1xz0").to_two_state()), "1000");
    let b = BitVec::from_u64(8, 0xa5);
    let l = LogicVec::from_two_state(&b);
    assert_eq!(l, hex8(0xa5));
    assert!(l.as_ref().bval().iter().all(|&w| w == 0));
    let mut d = BitVec::zero(4);
    resize::to_two_state(d.as_mut(), lv("1xz0").as_ref());
    assert_eq!(bits2(&d), "1000");
    assert_eq!(
        resize::to_two_state_l64(L64::load(lv("1xz0").as_ref())),
        0b1000
    );
}

// ===========================================================================
// TC32 連結・複製 (§11.4.12, §11.4.12.1) — 幅 0 を含む (L-5)
// ===========================================================================

/// §11.4.12: 先頭の要素が MSB 側。§11.4.12.1: 幅 0 の要素は無視する。
#[test]
fn concat_11_4_12() {
    let e = LogicVec::zero(0);
    assert_eq!(
        bits(&LogicVec::concat(&[&lv("10xz"), &e, &lv("01")])),
        "10xz01"
    );
    assert_eq!(LogicVec::concat(&[&e]).width(), 0);
    let hi = LogicVec::from_u64(64, 0x0123_4567_89ab_cdef);
    let c = LogicVec::concat(&[&hi, &lv("1")]);
    assert_eq!(c.width(), 65);
    assert_eq!(c.bit(0), Logic::L1);
    assert_eq!(c.shr_const(1).resize(64, false), hi);
    assert_eq!(
        LogicVec::concat(&[&lv("1"), &LogicVec::zero(64)]),
        one_hot(65, &[64])
    );
    let z = lv("z");
    let zs: Vec<&LogicVec> = (0..129).map(|_| &z).collect();
    assert_eq!(LogicVec::concat(&zs), LogicVec::filled(129, Logic::Z));
    let c = LogicVec::concat(&[&LogicVec::filled(63, Logic::L1), &lv("x0z")]);
    assert_eq!(c.width(), 66);
    assert_eq!(bits(&c), format!("{}x0z", "1".repeat(63)));
    // 2 値
    let c2 = BitVec::concat(&[&bv("10"), &BitVec::zero(0), &bv("01")]);
    assert_eq!(bits2(&c2), "1001");
}

/// §11.4.12.1: 複製。`{0{x}}` は幅 0。
#[test]
fn replicate_11_4_12_1() {
    assert_eq!(bits(&lv("1z").replicate(3)), "1z1z1z");
    assert_eq!(lv("1z").replicate(0).width(), 0);
    assert_eq!(lv("1").replicate(129), LogicVec::filled(129, Logic::L1));
    let mut v = LogicVec::zero(65);
    v.set_bit(64, Logic::X);
    v.set_bit(3, Logic::L1);
    let r = v.replicate(2);
    assert_eq!(r.width(), 130);
    assert_eq!(r.resize(65, false), v);
    assert_eq!(r.shr_const(65).resize(65, false), v);
    assert_eq!(bits2(&bv("10").replicate(3)), "101010");
    let mut d = LogicVec::zero(6);
    concat::replicate(d.as_mut(), lv("1z").as_ref(), 3);
    assert_eq!(bits(&d), "1z1z1z");
}

// ===========================================================================
// TC33 選択とインデックス (§11.5.1)
// ===========================================================================

/// §11.5.1: 範囲外のビットは 4 値なら x。オフセットは LSB を 0 とする物理オフセット。
#[test]
fn select_11_5_1_read_out_of_range_is_x() {
    let src = lv("1010x01z");
    assert_eq!(bits(&src.get_slice(4, Index::Value(2))), "10x0");
    assert_eq!(bits(&src.get_slice(4, Index::Value(6))), "xx10");
    assert_eq!(bits(&src.get_slice(4, Index::Value(-2))), "1zxx");
    for off in [100, i64::MAX, i64::MIN] {
        assert_eq!(bits(&src.get_slice(4, Index::Value(off))), "xxxx");
    }
    assert_eq!(bits(&src.get_slice(4, Index::Unknown)), "xxxx");
    assert_eq!(src.get_bit(Index::Value(0)), Logic::Z);
    assert_eq!(src.get_bit(Index::Value(8)), Logic::X);
    assert_eq!(src.get_bit(Index::Unknown), Logic::X);
}

/// §11.5.1 と plan.md §3.5.1 規則 5 (決定 R-2): 2 値の範囲外・x インデックスは 0 [要VCS確認 V-9]。
#[test]
fn select_11_5_1_two_state_out_of_range_is_zero() {
    let src = bv("10100011");
    assert_eq!(bits2(&src.get_slice(4, Index::Value(6))), "0010");
    assert_eq!(bits2(&src.get_slice(4, Index::Unknown)), "0000");
    assert!(!src.get_bit(Index::Unknown));
    // 4 値のインデックスを index_value で解釈してから 2 値の選択に渡しても結果は 2 値の 0。
    let idx = select::index_value(lv("x10").as_ref(), false);
    assert_eq!(idx, Index::Unknown);
    let mut d = BitVec::filled(4, true);
    select::get_slice2(d.as_mut(), src.as_ref(), idx);
    assert_eq!(bits2(&d), "0000");
}

/// §11.5.1: 書き込みは範囲内のビットだけ。`Unknown` なら何も書かない。
#[test]
fn select_11_5_1_write() {
    let cases: [(Index, &str, &str); 4] = [
        (Index::Value(6), "1111", "11000000"),
        (Index::Value(-2), "1011", "00000010"),
        (Index::Unknown, "1111", "00000000"),
        (Index::Value(i64::MAX), "1111", "00000000"),
    ];
    for (idx, src, want) in cases {
        let mut d = LogicVec::zero(8);
        d.set_slice(idx, &lv(src));
        assert_eq!(bits(&d), want, "{idx:?}");
        let mut d2 = BitVec::zero(8);
        d2.set_slice(idx, &bv(src));
        assert_eq!(bits2(&d2), want, "2-state {idx:?}");
    }
    let mut d = LogicVec::zero(8);
    d.set_slice(Index::Value(i64::MIN), &lv("1111"));
    assert_eq!(d, LogicVec::zero(8));
}

/// §11.5.1: 129 ビットで `Value(126)` 幅 4 は bit 126..128 が範囲内、bit 129 は範囲外。
#[test]
fn select_11_5_1_word_boundary() {
    let mut src = LogicVec::zero(129);
    src.set_bit(126, Logic::L1);
    src.set_bit(128, Logic::Z);
    assert_eq!(bits(&src.get_slice(4, Index::Value(126))), "xz01");
    let mut d = LogicVec::zero(129);
    d.set_slice(Index::Value(126), &lv("1111"));
    let want: Vec<u32> = vec![126, 127, 128];
    assert_eq!(d, one_hot(129, &want));
}

/// §11.5.1: インデックスの解釈。x/z があれば Unknown、`i64` に収まらなければ飽和。
#[test]
fn index_value_11_5_1() {
    assert_eq!(lv("0101").index_value(false), Index::Value(5));
    assert_eq!(lv("1011").index_value(true), Index::Value(-5));
    assert_eq!(lv("1011").index_value(false), Index::Value(11));
    for s in [false, true] {
        assert_eq!(lv("01x0").index_value(s), Index::Unknown);
        assert_eq!(lv("01z0").index_value(s), Index::Unknown);
    }
    let p63 = one_hot(64, &[63]);
    assert_eq!(p63.index_value(false), Index::Value(i64::MAX));
    assert_eq!(p63.index_value(true), Index::Value(i64::MIN));
    let p100 = one_hot(128, &[100]);
    assert_eq!(p100.index_value(false), Index::Value(i64::MAX));
    // -2^100 (128 ビット、符号付き)
    let mut n100 = LogicVec::filled(128, Logic::L1);
    for i in 0..100 {
        n100.set_bit(i, Logic::L0);
    }
    assert_eq!(n100.index_value(true), Index::Value(i64::MIN));
    // 128 ビットの -1 (符号付き) は -1 に収まる
    assert_eq!(
        LogicVec::filled(128, Logic::L1).index_value(true),
        Index::Value(-1)
    );
    assert_eq!(
        select::index_value_l64(L64::new(0b1011, 0), 4, true),
        Index::Value(-5)
    );
}

// ===========================================================================
// TC34 混在カーネル (plan.md §3.5.1、決定 R-2) のうちシフト [要VCS確認 V-9]
// ===========================================================================

/// plan.md §3.5.1: 2 値のデータ × 4 値のシフト量 → 4 値の結果。x のシフト量なら全 x。
#[test]
fn mixed_shift_3_5_1_v9() {
    let b = BitVec::from_u64(8, 0xa5);
    let r = b.shl_logic(&LogicVec::from_u64(3, 1));
    assert_eq!(r, hex8(0x4a));
    assert!(r.as_ref().bval().iter().all(|&w| w == 0));
    assert_eq!(bits(&b.shl_logic(&lv("x01"))), "xxxxxxxx");
    assert_eq!(bits(&b.shr_logic(&lv("z00"))), "xxxxxxxx");
    assert_eq!(b.ashr_logic(&LogicVec::from_u64(3, 2), true), hex8(0xe9));
    assert_eq!(b.ashr_logic(&LogicVec::from_u64(3, 2), false), hex8(0x29));
    assert_eq!(bits(&b.ashr_logic(&lv("xxx"), true)), "xxxxxxxx");
    let mut d = LogicVec::zero(8);
    shift::ashr_bits(d.as_mut(), b.as_ref(), lv("010").as_ref(), true);
    assert_eq!(d, hex8(0xe9));
    assert_eq!(
        shift::shl_bits_l64(0xa5, 8, L64::new(0b001, 0b100)),
        L64::all_x(8)
    );
}

// ===========================================================================
// TC36 $clog2 (§20.8.1)
// ===========================================================================

/// §20.8.1: `$clog2(0)` = 0 [要VCS確認 V-10]、`$clog2(1)` = 0、2 のべきの前後、多倍長。
#[test]
fn clog2_20_8_1() {
    let c = |v: &LogicVec| v.clog2();
    assert_eq!(c(&LogicVec::zero(8)), Some(0));
    assert_eq!(c(&hex8(1)), Some(0));
    assert_eq!(c(&hex8(2)), Some(1));
    assert_eq!(c(&hex8(3)), Some(2));
    assert_eq!(c(&hex8(4)), Some(2));
    assert_eq!(c(&hex8(5)), Some(3));
    for k in 2..=62u32 {
        let p = 1u64 << k;
        assert_eq!(c(&LogicVec::from_u64(64, p - 1)), Some(k));
        assert_eq!(c(&LogicVec::from_u64(64, p)), Some(k));
        assert_eq!(c(&LogicVec::from_u64(64, p + 1)), Some(k + 1));
    }
    assert_eq!(c(&LogicVec::from_u64(64, 1 << 63)), Some(63));
    assert_eq!(c(&LogicVec::from_u64(64, (1 << 63) + 1)), Some(64));
    assert_eq!(c(&LogicVec::from_u64(64, u64::MAX)), Some(64));
    assert_eq!(c(&one_hot(65, &[64])), Some(64));
    assert_eq!(c(&one_hot(65, &[64, 0])), Some(65));
    let below: Vec<u32> = (0..100).collect();
    assert_eq!(c(&one_hot(128, &below)), Some(100));
    assert_eq!(c(&one_hot(128, &[100])), Some(100));
    assert_eq!(c(&one_hot(128, &[100, 0])), Some(101));
    assert_eq!(c(&LogicVec::filled(128, Logic::L1)), Some(128));
    // 符号付きの型の 8'h80 も符号なしの 128 として扱う
    assert_eq!(c(&hex8(0x80)), Some(7));
    assert_eq!(c(&lv("1")), Some(0));
    assert_eq!(c(&lv("01x0")), None);
    assert_eq!(c(&lv("z000")), None);
    // 2 値版
    assert_eq!(BitVec::from_u64(8, 5).clog2(), 3);
    assert_eq!(math::clog2_2(BitVec::zero(3).as_ref()), 0);
    assert_eq!(math::clog2_l64(L64::new(4, 0)), Some(2));
}

// ===========================================================================
// TC53 上限近くの幅のスモークテスト (R-27、定期ジョブでだけ実行)
// ===========================================================================

/// TC53: 幅 2^20 で `1 << (2^20 - 1)`、連結、`$clog2`。
#[test]
#[ignore]
fn smoke_width_2_20() {
    let w = 1u32 << 20;
    let r = LogicVec::from_u64(w, 1).shl_const(w as u64 - 1);
    assert_eq!(r, one_hot(w, &[w - 1]));
    assert_eq!(r.clog2(), Some(w - 1));
    let hi = LogicVec::filled(w / 2, Logic::L1);
    let lo = LogicVec::zero(w / 2);
    let c = LogicVec::concat(&[&hi, &lo]);
    assert_eq!(c.width(), w);
    assert_eq!(c.get_slice(w / 2, Index::Value((w / 2) as i64)), hi);
    assert_eq!(c.get_slice(w / 2, Index::Value(0)), lo);
}
