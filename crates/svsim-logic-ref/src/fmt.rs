//! 表示用フォーマット `%b %o %h %d` (plan.md §3.9、IEEE 1800-2023 §21.2.1)。
//!
//! 2/8/16 進は、LSB 側からビットを桁ごとに集めて規則を素朴に適用する。
//! 10 進は `BigInt::to_str_radix` を使う。

use num_bigint::BigInt;
use num_traits::One;

use crate::value::{RefLogic, RefVec};

/// 表示の基数。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefFmtRadix {
    /// `%b`。
    Bin,
    /// `%o`。
    Oct,
    /// `%d`。
    Dec,
    /// `%h` / `%x`。
    Hex,
}

/// 表示の幅。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefFieldWidth {
    /// 既定の自動幅 (`%d` 等)。
    Auto,
    /// 最小の幅 (`%0d` は `width = 0`、`%5d` は `width = 5`、`%05d` は `zero_pad`)。
    Min {
        /// 最小の文字数。
        width: u32,
        /// 0 で埋める (`%05d`)。
        zero_pad: bool,
    },
}

/// 未知のビットを含むビット群を 1 文字で表す (§21.2.1 の x/z の表示規則、plan.md §3.9 規則 1, 2)。
/// 全部 x なら `x`、全部 z なら `z`、一部が x なら `X`、x が無く一部が z なら `Z`。
/// 未知が無ければ `None`。
fn unknown_char(bits: &[RefLogic]) -> Option<char> {
    if bits.iter().all(|b| *b == RefLogic::X) {
        Some('x')
    } else if bits.iter().all(|b| *b == RefLogic::Z) {
        Some('z')
    } else if bits.contains(&RefLogic::X) {
        Some('X')
    } else if bits.contains(&RefLogic::Z) {
        Some('Z')
    } else {
        None
    }
}

/// 2/8/16 進の全桁 (先頭の 0 を含む、MSB 側の桁が先頭)。
fn power_of_two_digits(x: &RefVec, bits_per_digit: usize) -> Vec<char> {
    let ndigits = x.width().div_ceil(bits_per_digit);
    let mut out = Vec::with_capacity(ndigits);
    for d in (0..ndigits).rev() {
        let lo = d * bits_per_digit;
        let hi = (lo + bits_per_digit).min(x.width()); // 最上位の桁は端数 (存在するビットだけ)
        let group = &x.0[lo..hi];
        let c = match unknown_char(group) {
            Some(c) => c,
            None => {
                let v = group.iter().enumerate().fold(0u32, |acc, (k, b)| {
                    acc | (u32::from(*b == RefLogic::One) << k)
                });
                char::from_digit(v, 16).expect("digit < 16") // 小文字の a-f
            }
        };
        out.push(c);
    }
    out
}

/// 10 進の自動幅 (plan.md §3.9 規則 3): 符号なしは `2^w - 1` の桁数、符号付きは `-2^(w-1)` の文字数。
fn decimal_auto_width(w: usize, signed: bool) -> usize {
    if signed {
        let min: BigInt = -(BigInt::one() << (w.saturating_sub(1)));
        min.to_str_radix(10).len()
    } else {
        let max: BigInt = (BigInt::one() << w) - 1;
        max.to_str_radix(10).len()
    }
}

fn pad_left(s: String, width: usize, c: char) -> String {
    let n = s.chars().count();
    if n >= width {
        return s;
    }
    let mut out: String = std::iter::repeat_n(c, width - n).collect();
    out.push_str(&s);
    out
}

/// 整数を表示用に書式化する (plan.md §3.9)。
pub fn format_int(x: &RefVec, radix: RefFmtRadix, signed: bool, width: RefFieldWidth) -> String {
    let bpd = match radix {
        RefFmtRadix::Bin => Some(1),
        RefFmtRadix::Oct => Some(3),
        RefFmtRadix::Hex => Some(4),
        RefFmtRadix::Dec => None,
    };
    match bpd {
        Some(bpd) => {
            let digits = power_of_two_digits(x, bpd);
            match width {
                // 規則 3: 自動幅は全桁 (先頭は 0 で埋まる)。
                RefFieldWidth::Auto => digits.into_iter().collect(),
                RefFieldWidth::Min { width, .. } => {
                    // 規則 4: 先頭の '0' の桁を取る。x/z の桁で止まる。少なくとも 1 桁残す。
                    let first = digits
                        .iter()
                        .position(|c| *c != '0')
                        .unwrap_or(digits.len().saturating_sub(1));
                    let min: String = digits[first..].iter().collect();
                    // 規則 5: %b %o %h は 0 で埋める [要VCS確認 V-7]。
                    pad_left(min, width as usize, '0')
                }
            }
        }
        None => {
            let body = match unknown_char(&x.0) {
                Some(c) => c.to_string(),
                None => x
                    .to_bigint(signed)
                    .expect("no unknown bits")
                    .to_str_radix(10),
            };
            match width {
                // 規則 3: 右詰めで空白を埋める。
                RefFieldWidth::Auto => pad_left(body, decimal_auto_width(x.width(), signed), ' '),
                // 規則 5: 空白で埋める。zero_pad なら 0 で埋める [要VCS確認 V-7]。
                // 負の値の 0 埋めは、C の printf と同じく符号の後ろを埋める (`-0005`)。
                // plan.md には明記が無い暫定の解釈。
                RefFieldWidth::Min {
                    width,
                    zero_pad: true,
                } => match body.strip_prefix('-') {
                    Some(mag) => {
                        let w = (width as usize).saturating_sub(1);
                        format!("-{}", pad_left(mag.to_string(), w, '0'))
                    }
                    None => pad_left(body, width as usize, '0'),
                },
                RefFieldWidth::Min {
                    width,
                    zero_pad: false,
                } => pad_left(body, width as usize, ' '),
            }
        }
    }
}

/// `itoa` `hextoa` `octtoa` `bintoa` (§6.16.11〜§6.16.14)。`%0d %0h %0o %0b` と同じ書式。
pub fn itoa(x: &RefVec, signed: bool, radix: RefFmtRadix) -> Vec<u8> {
    format_int(
        x,
        radix,
        signed,
        RefFieldWidth::Min {
            width: 0,
            zero_pad: false,
        },
    )
    .into_bytes()
}
