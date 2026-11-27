//! 表示用フォーマット `%b %o %h %d` と itoa 系 (IEEE 1800-2023 §21.2.1.2〜§21.2.1.4、§6.16.11〜
//! §6.16.14、plan.md §3.9)。
//!
//! 規則 (plan.md §3.9。§21.2.1.3/§21.2.1.4 の番号は未検証):
//! 1. `%b %o %h`: LSB 側から 1/3/4 ビットずつを 1 桁にする。最上位の桁は端数で、存在するビットだけで
//!    判定する。全ビット X → `x`、全ビット Z → `z`、一部が X → `X`、X が無く一部が Z → `Z`
//!    [要VCS確認 V-13]、全ビット既知 → 数字 (16 進は小文字)。
//! 2. `%d`: 値全体で同じ判定をする。全ビット既知なら 10 進 (符号付きで負なら `-`)。
//! 3. 自動幅 ([`FieldWidth::Auto`]): その幅の値の最大の表示の文字数。`%b %o %h` は先頭を 0 で、
//!    `%d` は空白で埋める。`%d` の文字数は、符号なしなら `2^w - 1` の桁数、符号付きなら
//!    `-2^(w-1)` の文字数。
//! 4. `Min { width: 0, .. }` (`%0d` 等): 先頭の 0 や空白を付けない最小の表示。`%0h` は X/Z の桁で止まる。
//! 5. `Min { width: n, zero_pad }`: 最小表示の左に埋め文字を足して n 文字にする (切り詰めない、
//!    自動幅には戻さない。Q-8)。`%d` は空白 (`zero_pad` なら 0)、`%b %o %h` は常に 0。
//!    負数の 0 埋めは符号を先頭に置く (`-0005`、Q-7)。X/Z の表示も同じ埋め文字で埋める
//!    (`000xx`、`0000x`、Q-7) [要VCS確認 V-7]。
//!
//! # ヒープ確保 (test-plan Q-9)
//! 呼び出し側が `out.reserve(format_len_bound(w, radix))` (`Min { width: n, .. }` なら
//! `n` との大きい方) と [`Scratch::reserve_for`] を済ませていれば、[`format_int`] と
//! [`format_int2`] は確保しない。10 進の変換は [`Scratch`] に被除数の写しと 10^19 ごとの
//! 桁の塊を置く。
//!
//! 幅 0 の値 (連結の外では呼び出し側の誤り) は panic せず、`%b %o %h` は空、`%d` は `0` を表示する。

use crate::layout::words_for;
use crate::scratch::Scratch;
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitRef, LogicRef};
use crate::words::{self, read_bits};

/// 表示の基数。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FmtRadix {
    /// `%b`。
    Bin,
    /// `%o`。
    Oct,
    /// `%d`。
    Dec,
    /// `%h` (`%x` も同じ)。
    Hex,
}

/// 表示の幅 (plan.md §3.9、R-19)。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum FieldWidth {
    /// 既定の自動幅 (`%d` 等)。
    Auto,
    /// 最小の文字数。`%0d` は `width = 0`、`%5d` は `width = 5`、`%05d` は `zero_pad = true`。
    Min {
        /// 最小の文字数。
        width: u32,
        /// 0 で埋める (`%05d`)。`%b %o %h` は指定によらず 0 で埋める。
        zero_pad: bool,
    },
}

/// 10^19 (u64 に入る最大の 10 のべき)。
const TEN19: u64 = 10_000_000_000_000_000_000;

/// floor(log10(2) · 2^64)。2^k の 10 進の桁数を整数演算で求めるのに使う。
/// `k <= 2^24` (MAX_WIDTH) のすべてで floor(k·log10(2)) と一致することを、多倍長の計算で確かめてある。
const LOG10_2_Q64: u128 = 0x4d10_4d42_7de7_fbcc;

/// 2^k の 10 進の桁数 (= 2^k - 1 の桁数。k >= 1 では 2^k は 10 のべきではないため。k = 0 は 1)。
#[inline]
fn pow2_decimal_digits(k: u32) -> usize {
    ((u128::from(k) * LOG10_2_Q64) >> 64) as usize + 1
}

/// `%d` の自動幅 (規則 3)。符号なしは `2^w - 1` の桁数、符号付きは `-2^(w-1)` の文字数。
fn decimal_auto_width(w: u32, signed: bool) -> usize {
    if signed {
        1 + pow2_decimal_digits(w.max(1) - 1)
    } else {
        pow2_decimal_digits(w)
    }
}

/// 2/8/16 進の 1 桁のビット数。
#[inline]
fn bits_per_digit(radix: FmtRadix) -> Option<u32> {
    match radix {
        FmtRadix::Bin => Some(1),
        FmtRadix::Oct => Some(3),
        FmtRadix::Hex => Some(4),
        FmtRadix::Dec => None,
    }
}

/// 幅 `width` の値を基数 `radix` で表示したときの文字数の上限 (test-plan Q-9)。
///
/// [`FieldWidth::Auto`] と `Min { width: 0, .. }` のどの値 (符号付き/符号なし、X/Z を含む) の表示も
/// この長さ以下。`Min { width: n, .. }` では `n` との大きい方が上限になる。
pub fn format_len_bound(width: u32, radix: FmtRadix) -> usize {
    match bits_per_digit(radix) {
        Some(bpd) => width.div_ceil(bpd) as usize,
        // 符号付きの自動幅は符号なしの自動幅以上 (2^w の桁数 <= 2^(w-1) の桁数 + 1)。
        None => decimal_auto_width(width, true),
    }
}

/// 値の読み取りの共通部分 (4 値と 2 値)。`b` が空なら 2 値。
#[derive(Clone, Copy)]
struct Planes<'a> {
    width: u32,
    a: &'a [u64],
    b: &'a [u64],
}

impl Planes<'_> {
    #[inline]
    fn bval(&self, off: usize, n: usize) -> u64 {
        if self.b.is_empty() {
            0
        } else {
            read_bits(self.b, off, n)
        }
    }

    /// 0 でない (a | b) の最上位ビットの位置 + 1。全部 0 なら 0。
    fn significant_bits(&self) -> u64 {
        let nw = self.a.len();
        (0..nw)
            .rev()
            .map(|i| (i, self.a[i] | self.b.get(i).copied().unwrap_or(0)))
            .find(|&(_, w)| w != 0)
            .map_or(0, |(i, w)| {
                64 * i as u64 + u64::from(64 - w.leading_zeros())
            })
    }

    /// 1 桁 (ビット `off` から `n` ビット) の文字 (規則 1)。
    #[inline]
    fn digit(&self, off: usize, n: usize) -> u8 {
        let a = read_bits(self.a, off, n);
        let b = self.bval(off, n);
        if b == 0 {
            return b"0123456789abcdef"[a as usize];
        }
        let full = if n == 64 { !0 } else { (1u64 << n) - 1 };
        unknown_char(a, b, full)
    }

    /// 値全体の未知の文字 (規則 2)。未知が無ければ `None`。
    fn unknown_char_all(&self) -> Option<u8> {
        if self.b.is_empty() || words::is_zero(self.b) {
            return None;
        }
        let nw = self.a.len();
        let (mut all_x, mut all_z, mut any_x) = (true, true, false);
        for i in 0..nw {
            let full = if i + 1 == nw {
                crate::layout::top_mask(self.width)
            } else {
                !0
            };
            let (a, b) = (self.a[i], self.b[i]);
            let x = a & b;
            let z = !a & b;
            all_x &= x == full;
            all_z &= z == full;
            any_x |= x != 0;
        }
        Some(if all_x {
            b'x'
        } else if all_z {
            b'z'
        } else if any_x {
            b'X'
        } else {
            b'Z'
        })
    }
}

/// 未知のビットを含むビット群 (`full` は存在するビットのマスク) の 1 文字。`b != 0` であること。
#[inline]
fn unknown_char(a: u64, b: u64, full: u64) -> u8 {
    let x = a & b;
    if x == full {
        b'x'
    } else if (!a & b) == full {
        b'z'
    } else if x != 0 {
        b'X'
    } else {
        b'Z'
    }
}

#[inline]
fn push_repeat(out: &mut String, c: char, n: usize) {
    for _ in 0..n {
        out.push(c);
    }
}

/// 2/8/16 進の表示 (規則 1, 3, 4, 5)。
fn format_pow2(out: &mut String, x: Planes<'_>, bpd: u32, width: FieldWidth) {
    let w = x.width as usize;
    let nd = x.width.div_ceil(bpd) as usize;
    let bpd = bpd as usize;
    // 表示する桁の数。
    let shown = match width {
        FieldWidth::Auto => nd,
        FieldWidth::Min { width: n, .. } => {
            // 規則 4: 0 の桁 (a = b = 0) を先頭から除く。少なくとも 1 桁は残す。
            let sig = x.significant_bits() as usize;
            let min = sig.div_ceil(bpd).max(1).min(nd);
            // 規則 5: 0 で埋める (自動幅に戻さない。Q-8)。
            push_repeat(out, '0', (n as usize).saturating_sub(min));
            min
        }
    };
    for d in (0..shown).rev() {
        let off = d * bpd;
        let n = bpd.min(w - off); // 最上位の桁は端数
        out.push(x.digit(off, n) as char);
    }
}

/// 10 進の表示 (規則 2, 3, 5)。既知の値の変換に `s` を使う。
fn format_dec(out: &mut String, x: Planes<'_>, signed: bool, width: FieldWidth, s: &mut Scratch) {
    let (pad_to, pad_char) = match width {
        FieldWidth::Auto => (decimal_auto_width(x.width, signed), ' '),
        FieldWidth::Min { width, zero_pad } => (width as usize, if zero_pad { '0' } else { ' ' }),
    };
    if let Some(c) = x.unknown_char_all() {
        push_repeat(out, pad_char, pad_to.saturating_sub(1));
        out.push(c as char);
        return;
    }

    // 既知の値: 絶対値を写し、10^19 で割って塊を下位から取り出す。
    let nw = words_for(x.width);
    let neg = signed && words::sign_bit(x.a, x.width);
    // 塊の数の上限: 10 進の桁数 / 19 + 1 (幅 0 でも 1)。
    let max_chunks = pow2_decimal_digits(x.width) / 19 + 1;
    let [mag, chunks] = s.parts([nw, max_chunks]);
    mag.copy_from_slice(x.a);
    if neg {
        // 幅 w の 2 の補数の絶対値。-2^(w-1) は 2^(w-1) になり、幅 w の符号なしに収まる。
        words::neg_assign(mag);
        crate::layout::normalize_plane(x.width, mag);
    }
    let mut nchunks = 0;
    let mut len = words::significant_words(mag);
    loop {
        chunks[nchunks] = words::divrem_small(&mut mag[..len], TEN19);
        nchunks += 1;
        while len > 0 && mag[len - 1] == 0 {
            len -= 1;
        }
        if len == 0 {
            break;
        }
    }
    let mut top_buf = [0u8; 20];
    let top = u64_decimal(chunks[nchunks - 1], &mut top_buf);
    let ndigits = top.len() + 19 * (nchunks - 1);
    let body = ndigits + usize::from(neg);
    let pad = pad_to.saturating_sub(body);
    if neg && pad_char == '0' {
        // 0 埋めの負数は符号を先頭に置く (Q-7)。
        out.push('-');
        push_repeat(out, '0', pad);
    } else {
        push_repeat(out, pad_char, pad);
        if neg {
            out.push('-');
        }
    }
    push_ascii(out, top);
    for i in (0..nchunks - 1).rev() {
        let mut buf = [0u8; 20];
        let d = u64_decimal(chunks[i], &mut buf);
        push_repeat(out, '0', 19 - d.len());
        push_ascii(out, d);
    }
}

/// `v` の 10 進表記 (先頭の 0 なし、0 は `"0"`) を `buf` の末尾に書き、その部分を返す。
fn u64_decimal(mut v: u64, buf: &mut [u8; 20]) -> &[u8] {
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    &buf[i..]
}

#[inline]
fn push_ascii(out: &mut String, s: &[u8]) {
    for &c in s {
        out.push(c as char);
    }
}

fn format_planes(
    out: &mut String,
    x: Planes<'_>,
    radix: FmtRadix,
    signed: bool,
    width: FieldWidth,
    s: &mut Scratch,
) {
    match bits_per_digit(radix) {
        Some(bpd) => format_pow2(out, x, bpd, width),
        None => format_dec(out, x, signed, width, s),
    }
}

/// 4 値の整数を表示用に書式化し、`out` の末尾に足す (plan.md §3.9、§21.2.1.2〜§21.2.1.4)。
///
/// `signed` は `%d` でだけ使う (2/8/16 進はビット列をそのまま表示する)。確保については
/// モジュールの説明を参照。
pub fn format_int(
    out: &mut String,
    x: LogicRef<'_>,
    radix: FmtRadix,
    signed: bool,
    width: FieldWidth,
    s: &mut Scratch,
) {
    let p = Planes {
        width: x.width(),
        a: x.aval(),
        b: x.bval(),
    };
    format_planes(out, p, radix, signed, width, s);
}

/// 2 値の [`format_int`]。
pub fn format_int2(
    out: &mut String,
    x: BitRef<'_>,
    radix: FmtRadix,
    signed: bool,
    width: FieldWidth,
    s: &mut Scratch,
) {
    let p = Planes {
        width: x.width(),
        a: x.words(),
        b: &[],
    };
    format_planes(out, p, radix, signed, width, s);
}

const MIN0: FieldWidth = FieldWidth::Min {
    width: 0,
    zero_pad: false,
};

/// `itoa` / `hextoa` / `octtoa` / `bintoa` (§6.16.11〜§6.16.14、plan.md §3.5(h))。
///
/// `%0d %0h %0o %0b` と同じ書式 ([`format_int`] に `Min { width: 0, .. }` を渡したもの) のバイト列。
/// 結果の `Vec` を作るので確保する。
pub fn itoa(x: LogicRef<'_>, signed: bool, radix: FmtRadix, s: &mut Scratch) -> Vec<u8> {
    let mut out = String::with_capacity(format_len_bound(x.width(), radix));
    format_int(&mut out, x, radix, signed, MIN0, s);
    out.into_bytes()
}

/// 2 値の [`itoa`]。
pub fn itoa2(x: BitRef<'_>, signed: bool, radix: FmtRadix, s: &mut Scratch) -> Vec<u8> {
    let mut out = String::with_capacity(format_len_bound(x.width(), radix));
    format_int2(&mut out, x, radix, signed, MIN0, s);
    out.into_bytes()
}

// ===========================================================================
// 層 3
// ===========================================================================

impl LogicVec {
    /// 表示用に書式化した文字列 ([`format_int`])。作業領域と結果の文字列を確保する。
    pub fn format(&self, radix: FmtRadix, signed: bool, width: FieldWidth) -> String {
        let mut out = String::new();
        format_int(
            &mut out,
            self.as_ref(),
            radix,
            signed,
            width,
            &mut Scratch::new(),
        );
        out
    }
}

impl BitVec {
    /// 表示用に書式化した文字列 ([`format_int2`])。作業領域と結果の文字列を確保する。
    pub fn format(&self, radix: FmtRadix, signed: bool, width: FieldWidth) -> String {
        let mut out = String::new();
        format_int2(
            &mut out,
            self.as_ref(),
            radix,
            signed,
            width,
            &mut Scratch::new(),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use FmtRadix::*;

    fn lv(s: &str) -> LogicVec {
        LogicVec::from_bits_str(s).unwrap()
    }

    fn min(width: u32, zero_pad: bool) -> FieldWidth {
        FieldWidth::Min { width, zero_pad }
    }

    /// 2^k の桁数を、ワード列で 2^k を作って 10^19 で割る方法と比べる。
    #[test]
    fn pow2_decimal_digits_matches_division() {
        for k in 0..=2048u32 {
            let mut x = vec![0u64; (k / 64 + 1) as usize];
            x[(k / 64) as usize] = 1 << (k % 64);
            let mut digits = 0;
            loop {
                words::divrem_small(&mut x, 10);
                digits += 1;
                if words::is_zero(&x) {
                    break;
                }
            }
            assert_eq!(pow2_decimal_digits(k), digits, "k = {k}");
        }
    }

    /// plan.md §3.9 の例 (§21.2.1.2〜§21.2.1.4)。
    #[test]
    fn examples_21_2_1() {
        let a = FieldWidth::Auto;
        assert_eq!(lv("10xz").format(Bin, false, a), "10xz");
        assert_eq!(lv("xxxx01z0").format(Hex, false, a), "xZ");
        assert_eq!(lv("zzzzxxxx").format(Hex, false, a), "zx");
        assert_eq!(lv("0x010000").format(Hex, false, a), "X0");
        assert_eq!(lv("xxxxxxxx").format(Dec, false, a), "  x");
        assert_eq!(lv("11111111").format(Dec, true, a), "  -1");
        assert_eq!(
            LogicVec::from_u64(32, 1).format(Dec, true, a),
            "          1"
        );
        assert_eq!(
            LogicVec::from_i64(32, -5).format(Dec, true, min(0, false)),
            "-5"
        );
    }

    /// 規則 4, 5 (Q-7, Q-8) [要VCS確認 V-7]。
    #[test]
    fn field_width_rules_4_5() {
        assert_eq!(
            LogicVec::from_i64(32, -5).format(Dec, true, min(5, true)),
            "-0005"
        );
        assert_eq!(lv("xxxxxxxx").format(Dec, false, min(5, true)), "0000x");
        assert_eq!(lv("xxxxxxxx").format(Hex, false, min(5, false)), "000xx");
        assert_eq!(
            LogicVec::from_u64(8, 0x0a).format(Hex, false, min(1, false)),
            "a"
        );
        assert_eq!(lv("xxxx00000001").format(Hex, false, min(0, false)), "x01");
        assert_eq!(lv("0000xxxx0001").format(Hex, false, min(0, false)), "x1");
        assert_eq!(LogicVec::zero(8).format(Hex, false, min(0, false)), "0");
        assert_eq!(LogicVec::zero(0).format(Hex, false, min(0, false)), "");
        assert_eq!(LogicVec::zero(0).format(Dec, false, min(0, false)), "0");
    }

    #[test]
    fn decimal_wide() {
        let a = FieldWidth::Auto;
        let ones = LogicVec::filled(128, crate::Logic::L1);
        assert_eq!(
            ones.format(Dec, false, a),
            "340282366920938463463374607431768211455"
        );
        let mut m = LogicVec::zero(128);
        m.set_bit(127, crate::Logic::L1);
        assert_eq!(
            m.format(Dec, true, a),
            "-170141183460469231731687303715884105728"
        );
        // 10^19 ちょうど (塊の境界で 0 埋めが要る)。
        let t = LogicVec::from_u64(64, TEN19);
        assert_eq!(t.format(Dec, false, min(0, false)), "10000000000000000000");
        assert_eq!(decimal_auto_width(1, true), 2);
        assert_eq!(decimal_auto_width(1, false), 1);
        assert_eq!(decimal_auto_width(64, true), 20);
        assert_eq!(decimal_auto_width(65, true), 21);
        assert_eq!(format_len_bound(0, Dec), 2);
    }
}
