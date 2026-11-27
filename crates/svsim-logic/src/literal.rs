//! リテラル解析 (IEEE 1800-2023 §5.7.1、plan.md §3.5(g))。
//!
//! 字句解析器が切り出した整数リテラルの部品 (サイズ、`'s`、基数、桁の並び) から値を作る。
//! 字句の切り出し (空白、`'` の位置、サイズの 10 進数の読み取り) はパーサの仕事で、ここでは扱わない。
//!
//! - [`parse_based`]: 基数付きリテラル `[size] ' [s] radix digits` (2/8/10/16 進)。
//! - [`parse_unsized_decimal`]: 基数なしの 10 進のサイズなし整数 (`123`)。
//! - 基数なしのサイズなし `'0 '1 'x 'z` は [`LogicVec::filled`] で文脈幅に展開する (幅は sema が決める)。
//!
//! 規則の要点 (plan.md §3.5(g)):
//! - `_` は区切りとして無視する。ただし桁の並びの先頭の `_` はエラー (§5.7.1 の文法。マネージャ決定)。
//! - `?` は z と同じ (§5.7.1)。x/z/? は大文字小文字を区別しない。
//! - 2/8/16 進: 桁から得たビット数がサイズより多ければ左を切り詰める。[`Literal::truncated`] は、
//!   落ちたビットに 0 以外 (1/x/z) があるときだけ立てる (決定 Q-3、Verilator と同じ。[要VCS確認 V-18])。
//!   少なければ左を 0 で埋める。ただし最左ビットが x/z ならその値で埋める。
//! - 10 進: 値がサイズに収まらなければ上位を切り詰める (`truncated`)。x/z/? は 1 桁だけのとき
//!   (後ろに `_` が続いてもよい) に許し、全ビットがその値になる (R-11)。
//! - サイズなし: 幅は 32 以上。32 ビットを超える値は必要な幅まで広げて
//!   [`Literal::widened_unsized`] を立てる (Verilator と同じ。[要VCS確認 V-11])。
//!   最左が x/z なら [`Literal::xz_extend`] を立てる (文脈幅への x/z の拡張は sema が行う)。
//! - 符号付き (`'s`) でも、リテラルの段階では 0 (または最左の x/z) で左を埋める。符号拡張は文脈で行う。
//! - サイズなし 10 進の 2^31〜2^32−1 は 32 ビットの符号付き (負の値) になる (暫定、マネージャ決定。
//!   [要VCS確認 V-11])。
//!
//! 結果の幅が 64 以下で、10 進の桁が 38 桁以下なら、ヒープ確保をしない。

use core::fmt;

use crate::layout::{MAX_WIDTH, normalize_plane, top_mask, words_for};
use crate::logic::Logic;
use crate::vec::LogicVec;
use crate::words;

/// リテラルの基数 (§5.7.1)。`str.atoi()` 系 ([`crate::string::atoi`]) でも使う。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Radix {
    /// 2 進 (`'b`)。
    Bin,
    /// 8 進 (`'o`)。
    Oct,
    /// 10 進 (`'d`)。
    Dec,
    /// 16 進 (`'h`)。
    Hex,
}

impl Radix {
    /// 基数の値 (2, 8, 10, 16)。
    pub const fn value(self) -> u32 {
        match self {
            Radix::Bin => 2,
            Radix::Oct => 8,
            Radix::Dec => 10,
            Radix::Hex => 16,
        }
    }

    /// 2/8/16 進の 1 桁のビット数。10 進は `None`。
    pub const fn bits_per_digit(self) -> Option<u32> {
        match self {
            Radix::Bin => Some(1),
            Radix::Oct => Some(3),
            Radix::Hex => Some(4),
            Radix::Dec => None,
        }
    }
}

/// 解析したリテラル。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Literal {
    /// 値。幅はサイズ付きならそのサイズ、サイズなしなら 32 以上。
    pub value: LogicVec,
    /// `'s` が付いている (基数なしの 10 進のサイズなし整数も真)。
    pub signed: bool,
    /// サイズが付いている。
    pub sized: bool,
    /// サイズなしで、最左ビットが x/z (文脈幅への x/z の拡張は sema が行う、§5.7.1)。
    pub xz_extend: bool,
    /// サイズに収まらず、0 以外のビットを左から切り詰めた (警告用。決定 Q-3、[要VCS確認 V-18])。
    pub truncated: bool,
    /// サイズなしで 32 ビットを超えたため幅を広げた (警告用。[要VCS確認 V-11])。
    pub widened_unsized: bool,
}

/// リテラル解析のエラー。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LiteralError {
    /// サイズが 0 (§5.7.1: サイズは 0 でない 10 進数)。
    ZeroSize,
    /// サイズ、またはサイズなしのリテラルに要る幅が [`MAX_WIDTH`] を超えた。
    TooWide {
        /// 要求された (または必要な) 幅。正確な値が分からないほど大きい場合は下限。
        width: u64,
    },
    /// 桁が 1 つも無い。
    NoDigits,
    /// 桁の並びが `_` で始まる (§5.7.1 の文法。マネージャ決定)。
    LeadingUnderscore,
    /// その基数で使えない文字。
    InvalidDigit {
        /// 桁の並びの中のバイト位置。
        index: usize,
        /// その文字。
        ch: char,
        /// 基数。
        radix: Radix,
    },
    /// 10 進の x/z/? が単独の 1 桁でない (`8'd1x`、`8'dxx` など。§5.7.1、R-11)。
    DecimalUnknownNotAlone {
        /// 問題の文字の、桁の並びの中のバイト位置。
        index: usize,
    },
    /// 基数なしの 10 進数 (`123`) に x/z/? がある (§5.7.1: unsigned_number は数字だけ)。
    UnknownInUnbasedDecimal,
}

impl fmt::Display for LiteralError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralError::ZeroSize => write!(f, "literal size must not be zero"),
            LiteralError::TooWide { width } => {
                write!(f, "literal width {width} exceeds MAX_WIDTH ({MAX_WIDTH})")
            }
            LiteralError::NoDigits => write!(f, "literal has no digits"),
            LiteralError::LeadingUnderscore => {
                write!(f, "a literal value must not start with '_'")
            }
            LiteralError::InvalidDigit { index, ch, radix } => write!(
                f,
                "invalid digit {ch:?} at byte {index} for radix {}",
                radix.value()
            ),
            LiteralError::DecimalUnknownNotAlone { index } => write!(
                f,
                "x/z digit in a decimal literal must be the only digit (byte {index})"
            ),
            LiteralError::UnknownInUnbasedDecimal => {
                write!(f, "x/z is not allowed in an unbased decimal number")
            }
        }
    }
}

impl std::error::Error for LiteralError {}

/// サイズなしリテラルの最小の幅 (§5.7.1)。
const UNSIZED_WIDTH: u32 = 32;

/// x/z/? の 1 文字。`?` は z (§5.7.1)。
fn unknown_digit(c: char) -> Option<Logic> {
    match c {
        'x' | 'X' => Some(Logic::X),
        'z' | 'Z' | '?' => Some(Logic::Z),
        _ => None,
    }
}

/// 2/8/16 進の 1 桁を、`bpd` ビットの aval/bval の組にする。
fn based_digit(c: char, radix: Radix, bpd: u32) -> Option<(u64, u64)> {
    let m = (1u64 << bpd) - 1;
    match unknown_digit(c) {
        Some(Logic::X) => Some((m, m)),
        Some(_) => Some((0, m)),
        None => c.to_digit(radix.value()).map(|d| (u64::from(d), 0)),
    }
}

/// 基数付きリテラル `[size] ' [s] radix digits` を解析する (§5.7.1、plan.md §3.5(g))。
///
/// - `size`: サイズ (`None` はサイズなし)。`Some(0)` はエラー。
/// - `signed`: `'s` が付いている。値には影響しない (リテラルの段階では 0 で左を埋める)。
/// - `digits`: 基数の後ろの桁の並び (`_` を含んでよいが、先頭は不可)。
///
/// ```
/// # use svsim_logic::literal::{parse_based, Radix};
/// let l = parse_based(Some(8), false, Radix::Hex, "x1").unwrap();
/// assert_eq!(l.value.to_bits_string(), "xxxx0001");
/// let l = parse_based(Some(4), false, Radix::Dec, "20").unwrap();
/// assert_eq!(l.value.to_u64(), Some(4));
/// assert!(l.truncated);
/// ```
pub fn parse_based(
    size: Option<u32>,
    signed: bool,
    radix: Radix,
    digits: &str,
) -> Result<Literal, LiteralError> {
    if size == Some(0) {
        return Err(LiteralError::ZeroSize);
    }
    match radix.bits_per_digit() {
        Some(bpd) => parse_pow2(size, signed, radix, bpd, digits),
        None => match scan_decimal(digits)? {
            DecScan::Unknown(u) => {
                let width = size.unwrap_or(UNSIZED_WIDTH);
                check_max(u64::from(width))?;
                Ok(Literal {
                    value: LogicVec::filled(width, u),
                    signed,
                    sized: size.is_some(),
                    xz_extend: size.is_none(),
                    truncated: false,
                    widened_unsized: false,
                })
            }
            DecScan::Known { sig_digits } => match size {
                Some(s) => {
                    check_max(u64::from(s))?;
                    let (value, truncated) = decimal_sized(digits, s);
                    Ok(Literal {
                        value,
                        signed,
                        sized: true,
                        xz_extend: false,
                        truncated,
                        widened_unsized: false,
                    })
                }
                None => {
                    let (value, widened) = decimal_unsized(digits, sig_digits)?;
                    Ok(Literal {
                        value,
                        signed,
                        sized: false,
                        xz_extend: false,
                        truncated: false,
                        widened_unsized: widened,
                    })
                }
            },
        },
    }
}

/// 基数なしの 10 進のサイズなし整数 (`123`) を解析する (§5.7.1)。
///
/// 符号付きで、幅は 32 以上 (32 ビットを超えれば必要な幅まで広げる。[要VCS確認 V-11])。
/// 2^31〜2^32−1 は 32 ビットに収まるので幅 32 のまま (符号付きとしては負。暫定)。
pub fn parse_unsized_decimal(digits: &str) -> Result<Literal, LiteralError> {
    match scan_decimal(digits)? {
        DecScan::Unknown(_) => Err(LiteralError::UnknownInUnbasedDecimal),
        DecScan::Known { sig_digits } => {
            let (value, widened) = decimal_unsized(digits, sig_digits)?;
            Ok(Literal {
                value,
                signed: true,
                sized: false,
                xz_extend: false,
                truncated: false,
                widened_unsized: widened,
            })
        }
    }
}

fn check_max(width: u64) -> Result<(), LiteralError> {
    if width > u64::from(MAX_WIDTH) {
        Err(LiteralError::TooWide { width })
    } else {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 2/8/16 進
// ---------------------------------------------------------------------------

/// 2/8/16 進。1 回目の走査で検査と幅の決定、2 回目 (LSB 側から) でビットを置く。
fn parse_pow2(
    size: Option<u32>,
    signed: bool,
    radix: Radix,
    bpd: u32,
    digits: &str,
) -> Result<Literal, LiteralError> {
    if digits.starts_with('_') {
        return Err(LiteralError::LeadingUnderscore);
    }
    // 1 回目: 検査、桁数、最左の桁、0 でない最上位ビットまでの幅 (x/z も 0 でないとみなす)。
    let mut count: u64 = 0;
    let mut top: Option<char> = None;
    let mut sig: u64 = 0;
    for (index, ch) in digits.char_indices() {
        if ch == '_' {
            continue;
        }
        let (a, b) =
            based_digit(ch, radix, bpd).ok_or(LiteralError::InvalidDigit { index, ch, radix })?;
        count += 1;
        top.get_or_insert(ch);
        if sig > 0 {
            sig += u64::from(bpd);
        } else {
            sig = u64::from(64 - (a | b).leading_zeros());
        }
    }
    let Some(top) = top else {
        return Err(LiteralError::NoDigits);
    };
    let nbits = count * u64::from(bpd);
    let (width, widened) = match size {
        Some(s) => (u64::from(s), false),
        None => (
            sig.max(u64::from(UNSIZED_WIDTH)),
            sig > u64::from(UNSIZED_WIDTH),
        ),
    };
    check_max(width)?;
    let width32 = width as u32;
    // 最左の桁が x/z なら、その値で左を埋める (§5.7.1)。
    let fill = unknown_digit(top);

    let mut value = LogicVec::zero(width32);
    let mut truncated = false;
    {
        let (pa, pb) = value.as_mut().into_planes();
        let mut off: u64 = 0;
        for ch in digits.chars().rev() {
            if ch == '_' {
                continue;
            }
            // 1 回目で検査済み。
            let Some((a, b)) = based_digit(ch, radix, bpd) else {
                unreachable!("digit validated in the first pass")
            };
            truncated |= put_bits(pa, width, off, a, bpd);
            truncated |= put_bits(pb, width, off, b, bpd);
            off += u64::from(bpd);
        }
        if let Some(u) = fill
            && nbits < width
        {
            if u.aval() {
                set_ones(pa, nbits, width);
            }
            set_ones(pb, nbits, width);
        }
    }
    Ok(Literal {
        value,
        signed,
        sized: size.is_some(),
        xz_extend: size.is_none() && fill.is_some(),
        truncated,
        widened_unsized: widened,
    })
}

/// `plane` のビット `off..off+len` (`len ≤ 4`) に `v` を OR で置く。`width` 以上の位置のビットは
/// 捨て、捨てたビットに 1 があれば真を返す。`plane` の該当ビットは 0 であること。
fn put_bits(plane: &mut [u64], width: u64, off: u64, v: u64, len: u32) -> bool {
    if off >= width {
        return v != 0;
    }
    let keep = (width - off).min(u64::from(len)) as u32;
    let low = v & ((1u64 << keep) - 1);
    let w = (off / 64) as usize;
    let s = (off % 64) as u32;
    plane[w] |= low << s;
    if s + keep > 64 {
        plane[w + 1] |= low >> (64 - s);
    }
    (v >> keep) != 0
}

/// `plane` のビット `lo..hi` を 1 にする。
fn set_ones(plane: &mut [u64], lo: u64, hi: u64) {
    let mut i = lo;
    while i < hi {
        let w = (i / 64) as usize;
        let s = (i % 64) as u32;
        let n = (hi - i).min(u64::from(64 - s)) as u32;
        let m = if n == 64 { !0 } else { ((1u64 << n) - 1) << s };
        plane[w] |= m;
        i += u64::from(n);
    }
}

// ---------------------------------------------------------------------------
// 10 進
// ---------------------------------------------------------------------------

enum DecScan {
    /// 単独の x/z/? (後ろに `_` だけが続く)。
    Unknown(Logic),
    /// 数字だけ。`sig_digits` は先頭の 0 を除いた桁数。
    Known { sig_digits: u64 },
}

/// 10 進の桁の並びを検査する (§5.7.1 の decimal_number の文法、R-11)。
fn scan_decimal(digits: &str) -> Result<DecScan, LiteralError> {
    let mut chars = digits.char_indices();
    let Some((_, first)) = chars.next() else {
        return Err(LiteralError::NoDigits);
    };
    if let Some(u) = unknown_digit(first) {
        // x/z/? の 1 桁の後ろには `_` だけが許される。
        for (index, ch) in chars {
            if ch != '_' {
                return Err(LiteralError::DecimalUnknownNotAlone { index });
            }
        }
        return Ok(DecScan::Unknown(u));
    }
    if first == '_' {
        return Err(LiteralError::LeadingUnderscore);
    }
    let mut sig_digits: u64 = 0;
    for (index, ch) in digits.char_indices() {
        match ch {
            '_' => {}
            '0' if sig_digits == 0 => {}
            '0'..='9' => sig_digits += 1,
            _ if unknown_digit(ch).is_some() => {
                return Err(LiteralError::DecimalUnknownNotAlone { index });
            }
            _ => {
                return Err(LiteralError::InvalidDigit {
                    index,
                    ch,
                    radix: Radix::Dec,
                });
            }
        }
    }
    Ok(DecScan::Known { sig_digits })
}

/// 10^k (k = 0..=19)。
const POW10: [u64; 20] = {
    let mut t = [1u64; 20];
    let mut i = 1;
    while i < 20 {
        t[i] = t[i - 1] * 10;
        i += 1;
    }
    t
};

/// 検査済みの 10 進の桁を `x` に積む (`x = x·10^k + chunk` を 19 桁ずつ。`words::mul_small_add`)。
/// 値は `2^(64·len)` の剰余になる。最上位ワードからあふれたことがあれば真を返す。
fn accumulate_decimal(digits: &str, x: &mut [u64]) -> bool {
    let mut overflow = false;
    let mut chunk: u64 = 0;
    let mut k = 0usize;
    for c in digits.bytes() {
        if c == b'_' {
            continue;
        }
        chunk = chunk * 10 + u64::from(c - b'0');
        k += 1;
        if k == 19 {
            overflow |= words::mul_small_add(x, POW10[19], chunk) != 0;
            chunk = 0;
            k = 0;
        }
    }
    if k > 0 {
        overflow |= words::mul_small_add(x, POW10[k], chunk) != 0;
    }
    overflow
}

/// サイズ付きの 10 進。値を `size` ビットの剰余にし、収まらなかったかを返す。
fn decimal_sized(digits: &str, size: u32) -> (LogicVec, bool) {
    let mut value = LogicVec::zero(size);
    let truncated;
    {
        let (pa, _) = value.as_mut().into_planes();
        // ワード列の長さでの剰余を計算する。途中であふれなければ値は正確なので、最上位ワードの
        // `size` ビット目以上を見れば「収まらない」が分かる。あふれたなら値は `2^(64·nw)` 以上。
        let overflow = accumulate_decimal(digits, pa);
        let top = pa[pa.len() - 1];
        truncated = overflow || (top & !top_mask(size)) != 0;
        normalize_plane(size, pa);
    }
    (value, truncated)
}

/// サイズなしの 10 進。幅は max(32, 値のビット数)。32 を超えたかを返す。
fn decimal_unsized(digits: &str, sig_digits: u64) -> Result<(LogicVec, bool), LiteralError> {
    // 値 ≥ 10^(sig_digits-1) > 2^(3·(sig_digits-1)) なので、これが MAX_WIDTH 以上なら幅が足りない。
    let lower_bits = sig_digits.saturating_sub(1).saturating_mul(3);
    check_max(lower_bits)?;
    // 値 < 10^sig_digits < 2^(sig_digits·10/3 + 1)。
    let nwords = (sig_digits * 10 / 3 + 1).div_ceil(64).max(1) as usize;
    let mut stack = [0u64; 2];
    let mut heap: Vec<u64>;
    let buf: &mut [u64] = if nwords <= stack.len() {
        &mut stack[..nwords]
    } else {
        heap = vec![0u64; nwords];
        &mut heap
    };
    let overflow = accumulate_decimal(digits, buf);
    debug_assert!(!overflow, "decimal_unsized: buffer too small");
    let need = words::bit_len(buf);
    let width = need.max(u64::from(UNSIZED_WIDTH));
    check_max(width)?;
    let width = width as u32;
    let mut value = LogicVec::zero(width);
    {
        let (pa, _) = value.as_mut().into_planes();
        let n = words_for(width).min(buf.len());
        pa[..n].copy_from_slice(&buf[..n]);
    }
    Ok((value, need > u64::from(UNSIZED_WIDTH)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(l: &Literal) -> String {
        assert!(l.value.as_ref().is_normalized());
        l.value.to_bits_string()
    }

    #[test]
    fn s5_7_1_hex_x_extend_left() {
        let l = parse_based(Some(12), false, Radix::Hex, "x1").unwrap();
        assert_eq!(bits(&l), "xxxxxxxx0001");
        assert!(!l.truncated && l.sized && !l.xz_extend);
    }

    #[test]
    fn s5_7_1_truncated_only_nonzero_q3() {
        // [要VCS確認 V-18]
        assert!(
            !parse_based(Some(8), false, Radix::Hex, "0ff")
                .unwrap()
                .truncated
        );
        assert!(
            parse_based(Some(4), false, Radix::Hex, "1f")
                .unwrap()
                .truncated
        );
        assert!(
            parse_based(Some(4), false, Radix::Hex, "z0")
                .unwrap()
                .truncated
        );
    }

    #[test]
    fn s5_7_1_put_bits_straddles_word() {
        // 8 進の 1 桁 (3 ビット) がワード境界 (63..66) をまたぐ。
        let l = parse_based(Some(66), false, Radix::Oct, "7_000000000000000000000").unwrap();
        assert_eq!(l.value.bit(63), Logic::L1);
        assert_eq!(l.value.bit(64), Logic::L1);
        assert_eq!(l.value.bit(65), Logic::L1);
        assert_eq!(l.value.bit(62), Logic::L0);
    }

    #[test]
    fn s5_7_1_decimal_large_unsized() {
        // 2^64 (20 桁) → 幅 65。[要VCS確認 V-11]
        let l = parse_unsized_decimal("18446744073709551616").unwrap();
        assert_eq!(l.value.width(), 65);
        assert!(l.widened_unsized);
        assert_eq!(l.value.bit(64), Logic::L1);
        // 40 桁 → ヒープの作業領域を使う経路。
        let l = parse_unsized_decimal("1000000000000000000000000000000000000000").unwrap();
        assert_eq!(l.value.width(), 130);
    }

    #[test]
    fn s5_7_1_errors() {
        assert_eq!(
            parse_based(Some(0), false, Radix::Bin, "1"),
            Err(LiteralError::ZeroSize)
        );
        assert_eq!(
            parse_based(Some(8), false, Radix::Bin, "_1"),
            Err(LiteralError::LeadingUnderscore)
        );
        assert_eq!(
            parse_based(Some(8), false, Radix::Dec, "_1"),
            Err(LiteralError::LeadingUnderscore)
        );
        assert_eq!(
            parse_based(Some(8), false, Radix::Hex, ""),
            Err(LiteralError::NoDigits)
        );
        assert_eq!(
            parse_based(Some(MAX_WIDTH + 1), false, Radix::Hex, "1"),
            Err(LiteralError::TooWide {
                width: u64::from(MAX_WIDTH) + 1
            })
        );
        assert!(matches!(
            parse_unsized_decimal(&"9".repeat(6_000_000)),
            Err(LiteralError::TooWide { .. })
        ));
        assert_eq!(
            parse_unsized_decimal("x"),
            Err(LiteralError::UnknownInUnbasedDecimal)
        );
        assert_eq!(
            parse_based(Some(8), false, Radix::Dec, "1x"),
            Err(LiteralError::DecimalUnknownNotAlone { index: 1 })
        );
        assert_eq!(
            parse_based(Some(8), false, Radix::Oct, "8"),
            Err(LiteralError::InvalidDigit {
                index: 0,
                ch: '8',
                radix: Radix::Oct
            })
        );
    }
}
