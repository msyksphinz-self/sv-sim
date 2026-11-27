//! 2 値のワード列プリミティブ (add/sub/neg/shift/mul_small_add/divrem_small/cmp/is_zero、
//! 算術 (M0-T5) 用の mul_lo/divrem_in_place)。crate 内専用。
//!
//! 値は `u64` のリトルエンディアンのワード列 (ワード 0 が最下位、plan.md §3.2) で、幅の情報を持たない。
//! 2 値カーネルの aval プレーンや、4 値の算術で「未知が無い」と分かったあとの aval プレーンに使う
//! (plan.md §3.3: 4 値の算術は X 検査のあと 2 値カーネルに委ねる)。
//!
//! 約束:
//! - 2 つ以上のワード列を受ける関数は、長さが等しいことを要求する (`assert_eq!`。crate 内の誤用の検出)。
//! - 結果はワード列の長さで切り詰める (2^(64·len) の剰余)。幅 `w` での正規化 (最上位ワードの
//!   `w` ビット目以上を 0 にする) は呼び出し側が [`normalize_plane`](crate::layout::normalize_plane)
//!   で行う。
//! - ヒープ確保をしない。作業領域が要るものは無い (作業領域は呼び出し側が [`Scratch`](crate::Scratch)
//!   から切り出して渡す方針。`scratch.rs` のモジュールの説明を参照)。
//!
//! 共有先: M0-T5 (算術)、M0-T7 (10 進リテラル: [`mul_small_add`])、M0-T8 (10 進表示: [`divrem_small`]
//! で 10^19 ずつ割る)。

// T5/T7/T8 が使うまでは、crate 内から呼ばれない関数がある。使われ始めたら外してよい。
#![allow(dead_code)]

use core::cmp::Ordering;

/// `dst = x + y`。桁あふれ (最上位ワードからの繰り上がり) を返す。
pub(crate) fn add(dst: &mut [u64], x: &[u64], y: &[u64]) -> bool {
    assert_eq!(dst.len(), x.len(), "words::add: length mismatch");
    assert_eq!(dst.len(), y.len(), "words::add: length mismatch");
    let mut carry = false;
    for ((d, &a), &b) in dst.iter_mut().zip(x).zip(y) {
        let (s, c) = a.carrying_add(b, carry);
        *d = s;
        carry = c;
    }
    carry
}

/// `x += y`。桁あふれを返す。
pub(crate) fn add_assign(x: &mut [u64], y: &[u64]) -> bool {
    assert_eq!(x.len(), y.len(), "words::add_assign: length mismatch");
    let mut carry = false;
    for (d, &b) in x.iter_mut().zip(y) {
        let (s, c) = d.carrying_add(b, carry);
        *d = s;
        carry = c;
    }
    carry
}

/// `x += v` (`v` は 1 ワードの値)。桁あふれを返す。
pub(crate) fn add_small_assign(x: &mut [u64], v: u64) -> bool {
    let mut carry = v;
    for d in x.iter_mut() {
        if carry == 0 {
            return false;
        }
        let (s, c) = d.overflowing_add(carry);
        *d = s;
        carry = c as u64;
    }
    carry != 0
}

/// `dst = x - y`。借り (x < y、符号なしとして) を返す。
pub(crate) fn sub(dst: &mut [u64], x: &[u64], y: &[u64]) -> bool {
    assert_eq!(dst.len(), x.len(), "words::sub: length mismatch");
    assert_eq!(dst.len(), y.len(), "words::sub: length mismatch");
    let mut borrow = false;
    for ((d, &a), &b) in dst.iter_mut().zip(x).zip(y) {
        let (s, c) = a.borrowing_sub(b, borrow);
        *d = s;
        borrow = c;
    }
    borrow
}

/// `x -= y`。借りを返す。
pub(crate) fn sub_assign(x: &mut [u64], y: &[u64]) -> bool {
    assert_eq!(x.len(), y.len(), "words::sub_assign: length mismatch");
    let mut borrow = false;
    for (d, &b) in x.iter_mut().zip(y) {
        let (s, c) = d.borrowing_sub(b, borrow);
        *d = s;
        borrow = c;
    }
    borrow
}

/// `x -= v` (`v` は 1 ワードの値)。借りを返す。
pub(crate) fn sub_small_assign(x: &mut [u64], v: u64) -> bool {
    let mut borrow = v;
    for d in x.iter_mut() {
        if borrow == 0 {
            return false;
        }
        let (s, c) = d.overflowing_sub(borrow);
        *d = s;
        borrow = c as u64;
    }
    borrow != 0
}

/// `dst = -x` (2 の補数、`0 - x`)。
pub(crate) fn neg(dst: &mut [u64], x: &[u64]) {
    assert_eq!(dst.len(), x.len(), "words::neg: length mismatch");
    // -x = !x + 1
    let mut carry = true;
    for (d, &a) in dst.iter_mut().zip(x) {
        let (s, c) = (!a).overflowing_add(carry as u64);
        *d = s;
        carry = c;
    }
}

/// `x = -x` (2 の補数)。
pub(crate) fn neg_assign(x: &mut [u64]) {
    let mut carry = true;
    for d in x.iter_mut() {
        let (s, c) = (!*d).overflowing_add(carry as u64);
        *d = s;
        carry = c;
    }
}

/// `dst = x << n` (論理シフト)。ワード列の長さを超えたビットは捨て、空いたビットは 0。
/// `n` がビット数 (`64·len`) 以上なら全 0。
pub(crate) fn shl(dst: &mut [u64], x: &[u64], n: u64) {
    assert_eq!(dst.len(), x.len(), "words::shl: length mismatch");
    let len = x.len();
    let ws = word_shift(n, len);
    let bs = (n % 64) as u32;
    for i in (0..len).rev() {
        dst[i] = shl_word(x, i, ws, bs);
    }
}

/// `x <<= n` (論理シフト、in-place)。
pub(crate) fn shl_assign(x: &mut [u64], n: u64) {
    let len = x.len();
    let ws = word_shift(n, len);
    let bs = (n % 64) as u32;
    // 上位から書けば、まだ読む下位のワードを壊さない。
    for i in (0..len).rev() {
        x[i] = shl_word(x, i, ws, bs);
    }
}

/// `dst = x >> n` (論理シフト)。空いたビットは 0。`n` がビット数以上なら全 0。
pub(crate) fn shr(dst: &mut [u64], x: &[u64], n: u64) {
    assert_eq!(dst.len(), x.len(), "words::shr: length mismatch");
    let len = x.len();
    let ws = word_shift(n, len);
    let bs = (n % 64) as u32;
    for (i, d) in dst.iter_mut().enumerate() {
        *d = shr_word(x, i, ws, bs);
    }
}

/// `x >>= n` (論理シフト、in-place)。
pub(crate) fn shr_assign(x: &mut [u64], n: u64) {
    let len = x.len();
    let ws = word_shift(n, len);
    let bs = (n % 64) as u32;
    // 下位から書けば、まだ読む上位のワードを壊さない。
    for i in 0..len {
        x[i] = shr_word(x, i, ws, bs);
    }
}

/// シフト量のワード単位の部分。`len` 以上 (全部はみ出す) なら `len` に丸める。
#[inline]
fn word_shift(n: u64, len: usize) -> usize {
    let ws = n / 64;
    if ws >= len as u64 { len } else { ws as usize }
}

/// `x << (64·ws + bs)` の `i` 番目のワード (`bs < 64`)。
#[inline]
fn shl_word(x: &[u64], i: usize, ws: usize, bs: u32) -> u64 {
    if i < ws {
        return 0;
    }
    let lo = x[i - ws] << bs;
    let hi = if bs != 0 && i > ws {
        x[i - ws - 1] >> (64 - bs)
    } else {
        0
    };
    lo | hi
}

/// `x >> (64·ws + bs)` の `i` 番目のワード (`bs < 64`)。
#[inline]
fn shr_word(x: &[u64], i: usize, ws: usize, bs: u32) -> u64 {
    let len = x.len();
    if i + ws >= len {
        return 0;
    }
    let lo = x[i + ws] >> bs;
    let hi = if bs != 0 && i + ws + 1 < len {
        x[i + ws + 1] << (64 - bs)
    } else {
        0
    };
    lo | hi
}

/// `x = x·m + a` (`m`, `a` は 1 ワードの値)。最上位からあふれたワードを返す。
///
/// 10 進リテラルの解析 (M0-T7) で、10^19 ごとの桁を積むのに使う。
pub(crate) fn mul_small_add(x: &mut [u64], m: u64, a: u64) -> u64 {
    let mut carry = a;
    for d in x.iter_mut() {
        let t = (*d as u128) * (m as u128) + carry as u128;
        *d = t as u64;
        carry = (t >> 64) as u64;
    }
    carry
}

/// `x = x / d` (符号なし、0 方向への切り捨て) とし、剰余を返す。
///
/// 10 進表示 (M0-T8) で、10^19 ずつ割って桁を取り出すのに使う。
///
/// # Panics
/// `d == 0` の場合 (crate 内の誤用。0 除算の意味論 (§11.4.3 の X) は呼び出し側で先に扱う)。
pub(crate) fn divrem_small(x: &mut [u64], d: u64) -> u64 {
    assert!(d != 0, "words::divrem_small: division by zero");
    let mut rem = 0u64;
    for w in x.iter_mut().rev() {
        let t = ((rem as u128) << 64) | *w as u128;
        *w = (t / d as u128) as u64;
        rem = (t % d as u128) as u64;
    }
    rem
}

/// 符号なしとして比べる。
pub(crate) fn cmp(x: &[u64], y: &[u64]) -> Ordering {
    assert_eq!(x.len(), y.len(), "words::cmp: length mismatch");
    for (a, b) in x.iter().rev().zip(y.iter().rev()) {
        match a.cmp(b) {
            Ordering::Equal => {}
            o => return o,
        }
    }
    Ordering::Equal
}

/// 幅 `width` の 2 の補数として比べる (§11.8.1: 両方が符号付きのときの比較)。
///
/// `x`, `y` は幅 `width` で正規化されていること (最上位ワードの `width` ビット目以上が 0)。
pub(crate) fn cmp_signed(x: &[u64], y: &[u64], width: u32) -> Ordering {
    assert_eq!(x.len(), y.len(), "words::cmp_signed: length mismatch");
    debug_assert_eq!(x.len(), crate::layout::words_for(width));
    if width == 0 {
        return Ordering::Equal;
    }
    match (sign_bit(x, width), sign_bit(y, width)) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        // 符号が同じなら、2 の補数のビット列は符号なしの大小と同じ順に並ぶ。
        _ => cmp(x, y),
    }
}

/// 幅 `width` の値の符号ビット (ビット `width - 1`)。`width == 0` なら偽。
pub(crate) fn sign_bit(x: &[u64], width: u32) -> bool {
    if width == 0 {
        return false;
    }
    let i = width - 1;
    (x[(i / 64) as usize] >> (i % 64)) & 1 != 0
}

/// 全ワードが 0 か。
pub(crate) fn is_zero(x: &[u64]) -> bool {
    x.iter().all(|&w| w == 0)
}

/// 0 でない最上位のビットの位置 + 1 (値を表すのに要るビット数)。値が 0 なら 0。
pub(crate) fn bit_len(x: &[u64]) -> u64 {
    match x.iter().rposition(|&w| w != 0) {
        Some(i) => 64 * i as u64 + (64 - x[i].leading_zeros()) as u64,
        None => 0,
    }
}

/// 0 でない最上位のワードの位置 + 1。値が 0 なら 0 (Knuth D の正規化などで使う)。
pub(crate) fn significant_words(x: &[u64]) -> usize {
    x.iter().rposition(|&w| w != 0).map_or(0, |i| i + 1)
}

/// `dst = x · y` の下位 `dst.len()` ワード (2^(64·len) の剰余)。
///
/// 乗算の下位ビットは符号の解釈に依らない (§11.4.3) ので、符号付きの乗算にもそのまま使える。
/// `dst` は `x`, `y` と別の領域 (借用規則で保証される)。計算量は O(len²/2)。
pub(crate) fn mul_lo(dst: &mut [u64], x: &[u64], y: &[u64]) {
    assert_eq!(dst.len(), x.len(), "words::mul_lo: length mismatch");
    assert_eq!(dst.len(), y.len(), "words::mul_lo: length mismatch");
    let n = dst.len();
    dst.fill(0);
    let ny = significant_words(y);
    for (i, &xi) in x.iter().enumerate().take(significant_words(x)) {
        if xi == 0 {
            continue;
        }
        // dst[i..] += xi · y (n - i ワードで切り詰め)
        let mut carry = 0u64;
        for j in 0..ny.min(n - i) {
            let t = (xi as u128) * (y[j] as u128) + dst[i + j] as u128 + carry as u128;
            dst[i + j] = t as u64;
            carry = (t >> 64) as u64;
        }
        if i + ny < n {
            // 前の行の繰り上がりが dst[i + ny] 以上に伝わっていることがあるので、足し込む。
            add_small_assign(&mut dst[i + ny..], carry);
        }
    }
}

/// Knuth の Algorithm D (TAOCP 2 巻 §4.3.1) による多倍長の符号なし除算。`u64` のワードと `u128` の
/// 中間値を使う。
///
/// 入力 (呼び出し側が用意する作業領域):
/// - `un`: 長さ `n + 1`。`un[..n]` に被除数を入れ、`un[n] = 0` にしておく。
/// - `vn`: 長さ `n`。除数 (0 でないこと)。
///
/// 出力:
/// - `q` (`Some` のとき、長さ `n`): 商。
/// - `un[..n]`: 剰余 (`un[n]` は 0)。
/// - `vn` は壊れる (正規化のためにずらす)。
///
/// 作業領域はこの 2 本だけで、ヒープ確保をしない。
///
/// # Panics
/// 長さが合わない場合、除数が 0 の場合 (crate 内の誤用。0 除算の意味論 (§11.4.3 の X) は呼び出し側で
/// 先に扱う)。
pub(crate) fn divrem_in_place(un: &mut [u64], vn: &mut [u64], mut q: Option<&mut [u64]>) {
    let n = vn.len();
    assert_eq!(
        un.len(),
        n + 1,
        "words::divrem_in_place: un must have n + 1 words"
    );
    debug_assert_eq!(un[n], 0, "words::divrem_in_place: un[n] must be 0");
    if let Some(q) = q.as_deref_mut() {
        assert_eq!(q.len(), n, "words::divrem_in_place: q length mismatch");
        q.fill(0);
    }
    let nv = significant_words(vn);
    assert!(nv != 0, "words::divrem_in_place: division by zero");
    let nu = significant_words(&un[..n]);
    if nu < nv {
        // 被除数 < 除数: 商 0、剰余 = 被除数 (un はそのまま)。
        return;
    }
    if nv == 1 {
        // 除数が 1 ワード: 上から 1 ワードずつ割る。
        let r = divrem_small(&mut un[..nu], vn[0]);
        if let Some(q) = q {
            q[..nu].copy_from_slice(&un[..nu]);
        }
        un.fill(0);
        un[0] = r;
        return;
    }

    // D1: 正規化。除数の最上位ワードの最上位ビットが 1 になるようにずらす。
    let s = vn[nv - 1].leading_zeros();
    shl_assign(&mut vn[..nv], s as u64);
    // un[nu] は 0 (nu <= n で、un[n] = 0) なので、あふれたビットはそこに入る。
    shl_assign(&mut un[..=nu], s as u64);

    let vtop = vn[nv - 1] as u128;
    let vsec = vn[nv - 2] as u128;
    let m = nu - nv;
    // D2〜D7: 商の各ワードを上から求める。
    for j in (0..=m).rev() {
        // D3: qhat の見積もり。un[j + nv] <= vtop なので qhat < 2^64 + 2。
        let num = ((un[j + nv] as u128) << 64) | un[j + nv - 1] as u128;
        let mut qhat = num / vtop;
        let mut rhat = num % vtop;
        // `||` の短絡で、乗算は qhat < 2^64 のときだけ行う (u128 であふれない)。
        while qhat >> 64 != 0 || qhat * vsec > ((rhat << 64) | un[j + nv - 2] as u128) {
            qhat -= 1;
            rhat += vtop;
            if rhat >> 64 != 0 {
                break;
            }
        }
        let qhat = qhat as u64;

        // D4: un[j..=j + nv] -= qhat · vn
        let mut mul_carry = 0u64;
        let mut borrow = false;
        for i in 0..nv {
            let p = (qhat as u128) * (vn[i] as u128) + mul_carry as u128;
            mul_carry = (p >> 64) as u64;
            let (d, b) = un[i + j].borrowing_sub(p as u64, borrow);
            un[i + j] = d;
            borrow = b;
        }
        let (d, b) = un[j + nv].borrowing_sub(mul_carry, borrow);
        un[j + nv] = d;

        // D5/D6: 引きすぎた (結果が負) なら、qhat を 1 減らして除数を 1 回足し戻す (add back)。
        let qj = if b {
            let mut carry = false;
            for i in 0..nv {
                let (s, c) = un[i + j].carrying_add(vn[i], carry);
                un[i + j] = s;
                carry = c;
            }
            un[j + nv] = un[j + nv].wrapping_add(carry as u64);
            qhat - 1
        } else {
            qhat
        };
        if let Some(q) = q.as_deref_mut() {
            q[j] = qj;
        }
    }

    // D8: 剰余を元に戻す (正規化のずらしを戻す)。剰余 < 除数なので un[..nv] に収まる。
    shr_assign(&mut un[..nv], s as u64);
    un[nv..].fill(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// テスト用: ワード列を u128 (len ≤ 2) にする。
    fn to_u128(x: &[u64]) -> u128 {
        assert!(x.len() <= 2);
        x.iter()
            .enumerate()
            .fold(0u128, |acc, (i, &w)| acc | (w as u128) << (64 * i))
    }

    fn from_u128(v: u128) -> [u64; 2] {
        [v as u64, (v >> 64) as u64]
    }

    /// 小さな決定的 PRNG (splitmix64)。
    fn rng(seed: &mut u64) -> u64 {
        *seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = *seed;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// 境界値を多く混ぜた 128 ビットの値。
    fn sample_u128(seed: &mut u64) -> u128 {
        const SPECIAL: [u128; 8] = [
            0,
            1,
            u64::MAX as u128,
            1 << 64,
            u128::MAX,
            u128::MAX - 1,
            1 << 127,
            (1 << 127) - 1,
        ];
        let r = rng(seed);
        if r.is_multiple_of(3) {
            SPECIAL[(r >> 8) as usize % SPECIAL.len()]
        } else {
            (rng(seed) as u128) << 64 | rng(seed) as u128
        }
    }

    /// 加減算・符号反転は 2^128 の剰余環の演算と一致する (§11.4.3: 結果は幅で切り詰める)。
    #[test]
    fn add_sub_neg_match_u128() {
        let mut seed = 1;
        for _ in 0..2000 {
            let a = sample_u128(&mut seed);
            let b = sample_u128(&mut seed);
            let (x, y) = (from_u128(a), from_u128(b));
            let mut d = [0u64; 2];

            let c = add(&mut d, &x, &y);
            assert_eq!((to_u128(&d), c), a.overflowing_add(b), "add {a:#x} {b:#x}");
            let mut t = x;
            assert_eq!(add_assign(&mut t, &y), c);
            assert_eq!(t, d);

            let c = sub(&mut d, &x, &y);
            assert_eq!((to_u128(&d), c), a.overflowing_sub(b), "sub {a:#x} {b:#x}");
            let mut t = x;
            assert_eq!(sub_assign(&mut t, &y), c);
            assert_eq!(t, d);

            neg(&mut d, &x);
            assert_eq!(to_u128(&d), a.wrapping_neg(), "neg {a:#x}");
            let mut t = x;
            neg_assign(&mut t);
            assert_eq!(t, d);

            let v = rng(&mut seed) >> (rng(&mut seed) % 64);
            let mut t = x;
            let c = add_small_assign(&mut t, v);
            assert_eq!((to_u128(&t), c), a.overflowing_add(v as u128));
            let mut t = x;
            let c = sub_small_assign(&mut t, v);
            assert_eq!((to_u128(&t), c), a.overflowing_sub(v as u128));
        }
    }

    #[test]
    fn add_sub_edge_cases() {
        // 繰り上がりが全ワードを伝わる。
        let mut x = [u64::MAX, u64::MAX, u64::MAX];
        assert!(add_small_assign(&mut x, 1));
        assert_eq!(x, [0, 0, 0]);
        assert!(sub_small_assign(&mut x, 1));
        assert_eq!(x, [u64::MAX; 3]);
        // 空のワード列 (幅 0)。
        let mut e: [u64; 0] = [];
        assert!(!add(&mut [], &[], &[]));
        assert!(!sub(&mut [], &[], &[]));
        assert!(add_small_assign(&mut e, 1));
        assert!(!add_small_assign(&mut e, 0));
        neg_assign(&mut e);
        // -0 = 0
        let mut z = [0u64; 3];
        neg_assign(&mut z);
        assert_eq!(z, [0; 3]);
    }

    #[test]
    #[should_panic(expected = "words::add: length mismatch")]
    fn add_rejects_length_mismatch() {
        let mut d = [0u64; 2];
        add(&mut d, &[1, 2], &[3]);
    }

    /// シフトは u128 のシフトと一致する。`n >= 128` は全 0 (§11.4.10 の論理シフト)。
    #[test]
    fn shifts_match_u128() {
        let mut seed = 7;
        let amounts: Vec<u64> = (0..=130)
            .chain([191, 192, 1000, u64::MAX / 2, u64::MAX - 63, u64::MAX])
            .collect();
        for _ in 0..50 {
            let a = sample_u128(&mut seed);
            let x = from_u128(a);
            for &n in &amounts {
                let want_l = if n >= 128 { 0 } else { a << n };
                let want_r = if n >= 128 { 0 } else { a >> n };
                let mut d = [!0u64; 2];
                shl(&mut d, &x, n);
                assert_eq!(to_u128(&d), want_l, "shl {a:#x} {n}");
                let mut t = x;
                shl_assign(&mut t, n);
                assert_eq!(to_u128(&t), want_l, "shl_assign {a:#x} {n}");
                shr(&mut d, &x, n);
                assert_eq!(to_u128(&d), want_r, "shr {a:#x} {n}");
                let mut t = x;
                shr_assign(&mut t, n);
                assert_eq!(to_u128(&t), want_r, "shr_assign {a:#x} {n}");
            }
        }
    }

    #[test]
    fn shifts_long_and_empty() {
        // 3 ワードで、ワード境界をまたぐシフト。
        let x = [0x8000_0000_0000_0001u64, 0, 0];
        let mut d = [0u64; 3];
        shl(&mut d, &x, 65);
        assert_eq!(d, [0, 2, 1]);
        shr(&mut d, &[0, 2, 1], 65);
        assert_eq!(d, [0x8000_0000_0000_0001, 0, 0]);
        shl(&mut d, &x, 128);
        assert_eq!(d, [0, 0, 0x8000_0000_0000_0001]);
        shl(&mut d, &x, 192);
        assert_eq!(d, [0, 0, 0]);
        let mut e: [u64; 0] = [];
        shl_assign(&mut e, 3);
        shr_assign(&mut e, u64::MAX);
    }

    /// mul_small_add / divrem_small は u128 の乗算・除算と一致する。
    #[test]
    fn mul_small_add_and_divrem_small_match_u128() {
        let mut seed = 11;
        let ds = [
            1u64,
            2,
            3,
            10,
            10_000_000_000_000_000_000,
            u64::MAX,
            1 << 63,
        ];
        for _ in 0..2000 {
            let a = sample_u128(&mut seed);
            let m = match rng(&mut seed) % 4 {
                0 => 10_000_000_000_000_000_000, // 10^19
                1 => 0,
                2 => u64::MAX,
                _ => rng(&mut seed),
            };
            let add = rng(&mut seed);
            let mut x = from_u128(a);
            let carry = mul_small_add(&mut x, m, add);
            // 192 ビットで正確な値を作って比べる。
            let lo = (a as u64 as u128) * m as u128 + add as u128;
            let mid = ((a >> 64) as u64 as u128) * m as u128 + (lo >> 64);
            assert_eq!(x, [lo as u64, mid as u64], "mul_small_add {a:#x} {m} {add}");
            assert_eq!(carry, (mid >> 64) as u64);

            let d = if rng(&mut seed).is_multiple_of(2) {
                ds[(rng(&mut seed) % ds.len() as u64) as usize]
            } else {
                rng(&mut seed) | 1
            };
            let mut x = from_u128(a);
            let r = divrem_small(&mut x, d);
            assert_eq!(to_u128(&x), a / d as u128, "divrem_small {a:#x} / {d}");
            assert_eq!(r as u128, a % d as u128);
        }
    }

    /// 10^19 で割って積み直すと元に戻る (M0-T7/T8 の使い方)。
    #[test]
    fn decimal_round_trip() {
        const TEN19: u64 = 10_000_000_000_000_000_000;
        let orig = [0x0123_4567_89AB_CDEFu64, u64::MAX, 0xFFFF, 1];
        let mut x = orig;
        let mut digits = Vec::new();
        while !is_zero(&x) {
            digits.push(divrem_small(&mut x, TEN19));
        }
        let mut y = [0u64; 4];
        for &d in digits.iter().rev() {
            assert_eq!(mul_small_add(&mut y, TEN19, d), 0);
        }
        assert_eq!(y, orig);
    }

    #[test]
    #[should_panic(expected = "division by zero")]
    fn divrem_small_rejects_zero() {
        divrem_small(&mut [1], 0);
    }

    #[test]
    fn compare() {
        assert_eq!(cmp(&[1, 2], &[2, 1]), Ordering::Greater);
        assert_eq!(cmp(&[2, 1], &[1, 2]), Ordering::Less);
        assert_eq!(cmp(&[5, 5], &[5, 5]), Ordering::Equal);
        assert_eq!(cmp(&[], &[]), Ordering::Equal);
        // 幅 8 の符号付き: 0x80 = -128 < 0x7f = 127 (§11.8.1)。
        assert_eq!(cmp_signed(&[0x80], &[0x7f], 8), Ordering::Less);
        assert_eq!(cmp_signed(&[0xff], &[0x80], 8), Ordering::Greater); // -1 > -128
        assert_eq!(cmp_signed(&[0xff], &[0xff], 8), Ordering::Equal);
        assert_eq!(cmp_signed(&[1], &[0], 1), Ordering::Less); // 1 ビットの 1 は -1
        // 幅 65: ビット 64 が符号。
        assert_eq!(cmp_signed(&[0, 1], &[u64::MAX, 0], 65), Ordering::Less);
        assert_eq!(cmp_signed(&[], &[], 0), Ordering::Equal);
        let mut seed = 3;
        for _ in 0..1000 {
            let a = sample_u128(&mut seed);
            let b = sample_u128(&mut seed);
            assert_eq!(cmp(&from_u128(a), &from_u128(b)), a.cmp(&b));
            assert_eq!(
                cmp_signed(&from_u128(a), &from_u128(b), 128),
                (a as i128).cmp(&(b as i128))
            );
        }
    }

    #[test]
    fn zero_and_lengths() {
        assert!(is_zero(&[]));
        assert!(is_zero(&[0, 0]));
        assert!(!is_zero(&[0, 1]));
        assert_eq!(bit_len(&[0, 0]), 0);
        assert_eq!(bit_len(&[1, 0]), 1);
        assert_eq!(bit_len(&[u64::MAX, 0]), 64);
        assert_eq!(bit_len(&[0, 1]), 65);
        assert_eq!(bit_len(&[0, 0, 1 << 63]), 192);
        assert_eq!(significant_words(&[0, 0]), 0);
        assert_eq!(significant_words(&[3, 0]), 1);
        assert_eq!(significant_words(&[0, 0, 1]), 3);
        assert!(sign_bit(&[0, 1], 65));
        assert!(!sign_bit(&[u64::MAX, 0], 65));
        assert!(!sign_bit(&[], 0));
    }
    /// mul_lo は u128 の乗算 (2^128 の剰余) と一致する (§11.4.3: 下位ビットだけを計算する)。
    #[test]
    fn mul_lo_matches_u128() {
        let mut seed = 21;
        for _ in 0..3000 {
            let a = sample_u128(&mut seed);
            let b = sample_u128(&mut seed);
            let mut d = [!0u64; 2];
            mul_lo(&mut d, &from_u128(a), &from_u128(b));
            assert_eq!(to_u128(&d), a.wrapping_mul(b), "mul_lo {a:#x} {b:#x}");
            // 1 ワード
            let mut d1 = [!0u64; 1];
            mul_lo(&mut d1, &[a as u64], &[b as u64]);
            assert_eq!(d1[0], (a as u64).wrapping_mul(b as u64));
        }
        // 3 ワード: (2^192 - 1)^2 mod 2^192 = 1
        let mut d = [0u64; 3];
        mul_lo(&mut d, &[!0; 3], &[!0; 3]);
        assert_eq!(d, [1, 0, 0]);
        mul_lo(&mut [], &[], &[]);
    }

    /// u128 の除算と比べる divrem_in_place の補助。
    fn check_divrem_u128(a: u128, b: u128) {
        let mut un = [a as u64, (a >> 64) as u64, 0];
        let mut vn = from_u128(b);
        let mut q = [!0u64; 2];
        divrem_in_place(&mut un, &mut vn, Some(&mut q));
        assert_eq!(to_u128(&q), a / b, "q {a:#x} / {b:#x}");
        assert_eq!(to_u128(&un[..2]), a % b, "r {a:#x} % {b:#x}");
        assert_eq!(un[2], 0);
        // 商を求めない版
        let mut un = [a as u64, (a >> 64) as u64, 0];
        let mut vn = from_u128(b);
        divrem_in_place(&mut un, &mut vn, None);
        assert_eq!(to_u128(&un[..2]), a % b);
    }

    /// Knuth D は u128 の除算と一致する (除数 1 ワード / 2 ワード、被除数 < 除数を含む)。
    #[test]
    fn divrem_in_place_matches_u128() {
        let mut seed = 33;
        for _ in 0..5000 {
            let a = sample_u128(&mut seed);
            let mut b = sample_u128(&mut seed);
            if rng(&mut seed).is_multiple_of(3) {
                b >>= rng(&mut seed) % 128;
            }
            if b == 0 {
                b = 1 + (rng(&mut seed) as u128);
            }
            check_divrem_u128(a, b);
        }
        // 境界: 被除数 < 除数、等しい、除数 1、除数の最上位ビットが立っている / 立っていない。
        for (a, b) in [
            (5u128, 7u128),
            (7, 7),
            (u128::MAX, 1),
            (u128::MAX, u128::MAX),
            (u128::MAX, 1 << 64),
            (u128::MAX, (1 << 64) + 1),
            (u128::MAX - 1, u128::MAX),
            (1 << 127, (1 << 64) | 1),
            (0, 3),
        ] {
            check_divrem_u128(a, b);
        }
    }

    /// Knuth D の補正 (D6 "add back") が起きる入力 (test-plan TC26)。q·v + r = u と r < v も確かめる。
    #[test]
    fn divrem_in_place_add_back() {
        // u = 256'h7fff_ffff_ffff_ffff_8000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000
        // v = 256'h8000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0000_0001
        let u = [0u64, 0, 0x8000_0000_0000_0000, 0x7fff_ffff_ffff_ffff];
        let v = [1u64, 0, 0x8000_0000_0000_0000, 0];
        let mut un = [u[0], u[1], u[2], u[3], 0];
        let mut vn = v;
        let mut q = [0u64; 4];
        divrem_in_place(&mut un, &mut vn, Some(&mut q));
        assert_eq!(q, [0xffff_ffff_ffff_fffe, 0, 0, 0]);
        assert_eq!(
            &un[..4],
            &[2, 0xffff_ffff_ffff_ffff, 0x7fff_ffff_ffff_ffff, 0]
        );
        let mut back = [0u64; 4];
        mul_lo(&mut back, &q, &v);
        assert!(!add_assign(&mut back, &un[..4]));
        assert_eq!(back, u);
        assert_eq!(cmp(&un[..4], &v), Ordering::Less);
    }

    /// 多ワードの乱数で q·v + r = u、r < v (性質、test-plan TC26)。
    #[test]
    fn divrem_in_place_property_multiword() {
        let mut seed = 99;
        for _ in 0..500 {
            let n = 1 + (rng(&mut seed) % 6) as usize;
            let mut u = [0u64; 6];
            let mut v = [0u64; 6];
            for w in u.iter_mut().take(n) {
                *w = match rng(&mut seed) % 4 {
                    0 => 0,
                    1 => !0,
                    _ => rng(&mut seed),
                };
            }
            let nv = 1 + (rng(&mut seed) % n as u64) as usize;
            for w in v.iter_mut().take(nv) {
                *w = match rng(&mut seed) % 4 {
                    0 => 0,
                    1 => !0,
                    2 => 1 << 63,
                    _ => rng(&mut seed),
                };
            }
            if is_zero(&v[..n]) {
                v[0] = 1;
            }
            let (u, v) = (&u[..n], &v[..n]);
            let mut un = [0u64; 7];
            un[..n].copy_from_slice(u);
            let mut vn = [0u64; 6];
            vn[..n].copy_from_slice(v);
            let mut q = [0u64; 6];
            divrem_in_place(&mut un[..=n], &mut vn[..n], Some(&mut q[..n]));
            assert_eq!(cmp(&un[..n], v), Ordering::Less, "r < v");
            let mut back = [0u64; 6];
            mul_lo(&mut back[..n], &q[..n], v);
            assert!(!add_assign(&mut back[..n], &un[..n]));
            assert_eq!(&back[..n], u, "q*v + r == u");
        }
    }

    #[test]
    #[should_panic(expected = "division by zero")]
    fn divrem_in_place_rejects_zero() {
        divrem_in_place(&mut [1, 2, 0], &mut [0, 0], None);
    }
}
