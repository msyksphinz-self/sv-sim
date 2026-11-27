//! 文字列との変換 (plan.md §3.5(h))。

use crate::literal::RefRadix;
use crate::value::{RefLogic, RefVec};

/// 文字列リテラル → 整数 (§5.9, §11.10, §11.10.3)。結果の幅は `width`。
///
/// 1 文字を 8 ビットにし、最後の文字を LSB 側に置く。`width` が広ければ左を 0 で埋め、
/// 狭ければ左 (先頭側の文字) を切り詰める。空の文字列は `"\0"` (8 ビットの 0) として扱う。
pub fn from_string_bytes(width: usize, bytes: &[u8]) -> RefVec {
    let bytes: &[u8] = if bytes.is_empty() { &[0] } else { bytes };
    // LSB 先頭のビット列: 最後の文字の bit0 が全体の bit0。
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for &b in bytes.iter().rev() {
        for k in 0..8 {
            bits.push(RefLogic::from_bool((b >> k) & 1 == 1));
        }
    }
    RefVec(
        (0..width)
            .map(|i| bits.get(i).copied().unwrap_or(RefLogic::Zero))
            .collect(),
    )
}

/// 整数 → `string` 型 (§6.16, §6.24.1)。MSB 側から 8 ビットずつ文字にする。
///
/// 幅が 8 の倍数でなければ左を 0 で埋める。値が 0 のバイトは捨てる。
/// X/Z のビットは 0 として扱う (暫定 [要VCS確認 V-5])。
pub fn to_string_bytes(x: &RefVec) -> Vec<u8> {
    let nbytes = x.width().div_ceil(8);
    let mut out = Vec::with_capacity(nbytes);
    for byte_idx in (0..nbytes).rev() {
        let mut b = 0u8;
        for k in 0..8 {
            let i = byte_idx * 8 + k;
            if i < x.width() && x.bit(i) == RefLogic::One {
                b |= 1 << k;
            }
        }
        if b != 0 {
            out.push(b);
        }
    }
    out
}

/// `str.atoi()` `atohex()` `atooct()` `atobin()` (§6.16.9)。結果は 32 ビット (2 値の値)。
///
/// 先頭からその基数の数字と `_` を読み、他の文字か終端で止まる。数字が無ければ 0。
/// 符号、先頭の空白、`0x` などの接頭辞は解釈しない。32 ビットを超える分は切り詰める
/// [要VCS確認 V-6]。
pub fn atoi(s: &[u8], radix: RefRadix) -> RefVec {
    let r = radix.value();
    let mut v: u64 = 0;
    for &c in s {
        if c == b'_' {
            continue;
        }
        match (c as char).to_digit(r) {
            // 下位 32 ビットだけを保てば、切り詰めた結果は同じ。
            Some(d) => v = (v * u64::from(r) + u64::from(d)) & 0xffff_ffff,
            None => break,
        }
    }
    RefVec::from_u64(32, v)
}
