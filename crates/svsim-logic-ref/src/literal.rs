//! 整数リテラルの解析 (plan.md §3.5(g)、IEEE 1800-2023 §5.7.1)。
//!
//! 1 文字ずつ桁を読み、ビットを `RefVec` に積む素朴な実装。

use num_bigint::BigUint;
use num_traits::Zero;

use crate::value::{RefLogic, RefVec};

/// リテラルの基数。
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum RefRadix {
    /// 2 進 (`'b`)。
    Bin,
    /// 8 進 (`'o`)。
    Oct,
    /// 10 進 (`'d`)。
    Dec,
    /// 16 進 (`'h`)。
    Hex,
}

impl RefRadix {
    /// 基数の値 (2, 8, 10, 16)。
    pub fn value(self) -> u32 {
        match self {
            RefRadix::Bin => 2,
            RefRadix::Oct => 8,
            RefRadix::Dec => 10,
            RefRadix::Hex => 16,
        }
    }

    /// 2/8/16 進の 1 桁のビット数。10 進は `None`。
    fn bits_per_digit(self) -> Option<usize> {
        match self {
            RefRadix::Bin => Some(1),
            RefRadix::Oct => Some(3),
            RefRadix::Hex => Some(4),
            RefRadix::Dec => None,
        }
    }
}

/// 解析したリテラル。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct RefLiteral {
    /// 値。幅はサイズ付きならそのサイズ、サイズなしなら 32 以上。
    pub value: RefVec,
    /// `'s` が付いている (または 10 進のサイズなし整数)。
    pub signed: bool,
    /// サイズが付いている。
    pub sized: bool,
    /// サイズなしで最左ビットが x/z。文脈幅への x/z の拡張は sema が行う。
    pub xz_extend: bool,
    /// サイズに収まらず左を切り詰めた (警告用)。
    pub truncated: bool,
    /// サイズなしで 32 ビットを超えたため幅を広げた (警告用) [要VCS確認 V-11]。
    pub widened_unsized: bool,
}

/// サイズなしリテラルの最小の幅 (§5.7.1)。
const UNSIZED_WIDTH: usize = 32;

/// 未知値の 1 文字 (`x` `z` `?`) を読む。`?` は z (§5.7.1)。
fn unknown_digit(c: char) -> Option<RefLogic> {
    match c {
        'x' | 'X' => Some(RefLogic::X),
        'z' | 'Z' | '?' => Some(RefLogic::Z),
        _ => None,
    }
}

/// 2/8/16 進の桁を LSB 先頭のビット列にする。
fn based_digit_bits(radix: RefRadix, bpd: usize, digits: &str) -> Result<Vec<RefLogic>, String> {
    let mut msb_first: Vec<RefLogic> = Vec::new();
    for (pos, c) in digits.chars().enumerate() {
        if c == '_' {
            if pos == 0 {
                return Err("a literal value must not start with '_'".to_string());
            }
            continue;
        }
        if let Some(u) = unknown_digit(c) {
            msb_first.extend(std::iter::repeat_n(u, bpd));
            continue;
        }
        let d = c
            .to_digit(radix.value())
            .ok_or_else(|| format!("invalid digit {c:?} for radix {}", radix.value()))?;
        for k in (0..bpd).rev() {
            msb_first.push(RefLogic::from_bool((d >> k) & 1 == 1));
        }
    }
    if msb_first.is_empty() {
        return Err("no digits".to_string());
    }
    msb_first.reverse();
    Ok(msb_first)
}

/// 10 進の桁。値か、単独の x/z/? (後ろに `_` が続いてもよい、R-11)。
enum DecValue {
    Known(BigUint),
    Unknown(RefLogic),
}

fn dec_value(digits: &str) -> Result<DecValue, String> {
    let mut chars = digits.chars();
    let Some(first) = chars.next() else {
        return Err("no digits".to_string());
    };
    if let Some(u) = unknown_digit(first) {
        // x/z/? の 1 桁の後ろには `_` だけが許される (§5.7.1 の decimal_number の文法)。
        if chars.all(|c| c == '_') {
            return Ok(DecValue::Unknown(u));
        }
        return Err("x/z digit in a decimal literal must be the only digit".to_string());
    }
    if first == '_' {
        return Err("a literal value must not start with '_'".to_string());
    }
    let mut v = BigUint::zero();
    for c in digits.chars() {
        if c == '_' {
            continue;
        }
        let d = c
            .to_digit(10)
            .ok_or_else(|| format!("invalid decimal digit {c:?}"))?;
        v = v * 10u32 + d;
    }
    Ok(DecValue::Known(v))
}

/// 最上位の「0 でない」ビット (x/z を含む) の位置 + 1。全部 0 なら 0。
fn significant_width(bits: &[RefLogic]) -> usize {
    bits.iter()
        .rposition(|b| *b != RefLogic::Zero)
        .map_or(0, |p| p + 1)
}

/// 基数付きリテラル `[size] ' [s] radix digits` を解析する (§5.7.1)。
///
/// - 2/8/16 進: 桁から得たビット数が size より多ければ左を切り詰める。落ちるビットに
///   0 以外 (1/x/z) があるときだけ `truncated` を立てる (Q-3、Verilator と同じ)。少なければ左を 0 で埋める。ただし最左ビットが x/z ならその値で埋める。
/// - 10 進: 値が size に収まらなければ上位を切り詰める (`truncated`)。x/z/? は 1 桁のときだけ許し、
///   全ビットがその値になる。
/// - サイズなし: 幅は 32 以上。32 ビットを超える値は必要な幅まで広げる (`widened_unsized`)
///   [要VCS確認 V-11]。最左ビットが x/z なら `xz_extend`。
/// - size が 0 ならエラー。
pub fn parse_based(
    size: Option<u32>,
    signed: bool,
    radix: RefRadix,
    digits: &str,
) -> Result<RefLiteral, String> {
    if size == Some(0) {
        return Err("literal size must not be zero".to_string());
    }
    // 桁から得た LSB 先頭のビット列と、拡張に使う値 (最左ビットが x/z ならその値、§5.7.1)。
    let (bits, fill): (Vec<RefLogic>, RefLogic) = match radix.bits_per_digit() {
        Some(bpd) => {
            let bits = based_digit_bits(radix, bpd, digits)?;
            let top = *bits.last().expect("non-empty");
            let fill = if top.is_unknown() {
                top
            } else {
                RefLogic::Zero
            };
            (bits, fill)
        }
        None => match dec_value(digits)? {
            DecValue::Known(v) => {
                let n = v.bits() as usize;
                (RefVec::from_biguint(n, &v).0, RefLogic::Zero)
            }
            // 1 ビットの x/z として扱い、拡張でその値を全ビットに広げる。
            DecValue::Unknown(u) => (vec![u], u),
        },
    };
    let n = bits.len();
    let (width, truncated, widened_unsized) = match size {
        Some(s) => {
            let s = s as usize;
            // 落ちるビット (位置 s 以上) に 0 以外 (1/x/z) があるときだけ切り詰めとみなす
            // (マネージャ決定 Q-3。Verilator と同じ挙動。V-リストに追加予定)。
            // 例: 3'h7, 4'h0F, 4'o17 は立たず、4'o37 や 4'hxF は立つ。
            // 10 進は `bits` が先頭の 0 を含まないので、「値が収まらない」(R-11) と同じ判定になる。
            let truncated = bits.iter().skip(s).any(|b| *b != RefLogic::Zero);
            (s, truncated, false)
        }
        None => {
            let need = significant_width(&bits);
            let w = need.max(UNSIZED_WIDTH);
            (w, false, need > UNSIZED_WIDTH)
        }
    };
    let value = RefVec(
        (0..width)
            .map(|i| if i < n { bits[i] } else { fill })
            .collect(),
    );
    Ok(RefLiteral {
        value,
        signed,
        sized: size.is_some(),
        xz_extend: size.is_none() && fill.is_unknown(),
        truncated,
        widened_unsized,
    })
}

/// 基数なしの 10 進のサイズなし整数 (`123`) を解析する (§5.7.1)。
/// 符号付きで、幅は 32 以上 (32 ビットを超えれば必要な幅まで広げる [要VCS確認 V-11])。
pub fn parse_unsized_decimal(digits: &str) -> Result<RefLiteral, String> {
    match dec_value(digits)? {
        DecValue::Unknown(_) => Err("x/z is not allowed in an unbased decimal number".to_string()),
        DecValue::Known(v) => {
            let need = v.bits() as usize;
            let width = need.max(UNSIZED_WIDTH);
            Ok(RefLiteral {
                value: RefVec::from_biguint(width, &v),
                signed: true,
                sized: false,
                xz_extend: false,
                truncated: false,
                widened_unsized: need > UNSIZED_WIDTH,
            })
        }
    }
}
