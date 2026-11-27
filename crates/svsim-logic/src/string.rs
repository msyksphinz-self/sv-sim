//! 文字列との変換と atoi 系 (IEEE 1800-2023 §5.9, §6.16、plan.md §3.5(h))。
//!
//! - [`from_string_bytes`]: 文字列リテラル → 整数 (§5.9, §11.10, §11.10.3)。
//! - [`to_string_bytes`]: 整数 → `string` 型 (§6.16, §6.24.1)。
//! - [`atoi`]: `str.atoi()` `atohex()` `atooct()` `atobin()` (§6.16.9)。
//!
//! `itoa` 系 (§6.16.11〜§6.16.14) は表示用フォーマットと同じ書式なので `fmt.rs` (M0-T8) に置く。
//! `string` 型そのもの (メソッド群、比較、連結) は M1/M3 のランタイムの仕事。

use crate::layout::normalize_plane;
use crate::literal::Radix;
use crate::vec::{BitVec, LogicVec};
use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

/// 文字列のバイト列を、最後の文字を LSB 側にしてワード列 `dst` (長さ `nw`) に詰める。
/// 空の文字列は `"\0"` と同じ (全 0) になる。幅での正規化は呼び出し側が行う。
fn pack_bytes(dst: &mut [u64], bytes: &[u8]) {
    let n = bytes.len();
    for (k, w) in dst.iter_mut().enumerate() {
        let mut v = 0u64;
        for j in 0..8 {
            let idx = k * 8 + j;
            if idx >= n {
                break;
            }
            v |= u64::from(bytes[n - 1 - idx]) << (8 * j);
        }
        *w = v;
    }
}

/// 文字列リテラル → 整数 (§5.9, §11.10, §11.10.3)。結果は `dst` の幅。
///
/// 1 文字を 8 ビットにし、最後の文字を LSB 側に置く (右詰め)。`dst` が広ければ左を 0 で埋め、
/// 狭ければ左 (先頭側の文字) を切り詰める。空の文字列 `""` は `"\0"` (8 ビットの 0) として扱う
/// (§11.10.3)。`dst` の幅を決めるのは sema (R-11)。X/Z は出ない。
pub fn from_string_bytes(dst: LogicMut<'_>, bytes: &[u8]) {
    let width = dst.width();
    let (a, b) = dst.into_planes();
    pack_bytes(a, bytes);
    normalize_plane(width, a);
    b.fill(0);
}

/// 2 値の [`from_string_bytes`] (`bit [N:0] v = "…";`)。
pub fn from_string_bytes2(dst: BitMut<'_>, bytes: &[u8]) {
    let width = dst.width();
    let a = dst.into_words();
    pack_bytes(a, bytes);
    normalize_plane(width, a);
}

/// 2 値のワード列 (幅 `width`、正規化済み) を MSB 側から 8 ビットずつ文字にし、`out` に足す。
/// `unknown` があれば、そのビットが 1 の位置は 0 として扱う。
fn push_bytes(width: u32, a: &[u64], unknown: Option<&[u64]>, out: &mut Vec<u8>) {
    let nbytes = (width as usize).div_ceil(8);
    for i in (0..nbytes).rev() {
        let w = i / 8;
        let s = (i % 8) * 8;
        let mut v = a[w];
        if let Some(b) = unknown {
            v &= !b[w];
        }
        let byte = (v >> s) as u8;
        // 値が 0 のバイト (`"\0"`) は捨てる (§6.16)。
        if byte != 0 {
            out.push(byte);
        }
    }
}

/// 整数 → `string` 型 (§6.16, §6.24.1)。MSB 側から 8 ビットずつ文字にする。
///
/// 幅が 8 の倍数でなければ左を 0 で埋める。値が 0 のバイトは捨てる。
/// X/Z のビットは 0 として扱う (暫定 [要VCS確認 V-5])。
pub fn to_string_bytes(x: LogicRef<'_>) -> Vec<u8> {
    let mut out = Vec::with_capacity((x.width() as usize).div_ceil(8));
    write_string_bytes(x, &mut out);
    out
}

/// [`to_string_bytes`] の結果を `out` の末尾に足す (`out` の容量が足りればヒープ確保をしない)。
pub fn write_string_bytes(x: LogicRef<'_>, out: &mut Vec<u8>) {
    push_bytes(x.width(), x.aval(), Some(x.bval()), out);
}

/// 2 値の [`to_string_bytes`]。
pub fn to_string_bytes2(x: BitRef<'_>) -> Vec<u8> {
    let mut out = Vec::with_capacity((x.width() as usize).div_ceil(8));
    write_string_bytes2(x, &mut out);
    out
}

/// 2 値の [`write_string_bytes`]。
pub fn write_string_bytes2(x: BitRef<'_>, out: &mut Vec<u8>) {
    push_bytes(x.width(), x.words(), None, out);
}

/// `str.atoi()` `atohex()` `atooct()` `atobin()` (§6.16.9) の値を `u32` で返す。
///
/// 先頭から、その基数の数字と `_` を読み、他の文字か終端で止まる。数字が無ければ 0。
/// 先頭の `_` も読み飛ばす (`"_12"` → 12、決定 Q-4)。符号 (`-` `+`)、先頭の空白、`0x` などの
/// 接頭辞は解釈しない (R-9)。32 ビットを超える分は黙って切り詰める。[要VCS確認 V-6]
pub fn atoi_u32(s: &[u8], radix: Radix) -> u32 {
    let r = radix.value();
    let mut v: u32 = 0;
    for &c in s {
        if c == b'_' {
            continue;
        }
        match char::from(c).to_digit(r) {
            // 2^32 の剰余で計算すれば、切り詰めた結果と同じになる。
            Some(d) => v = v.wrapping_mul(r).wrapping_add(d),
            None => break,
        }
    }
    v
}

/// `str.atoi()` 系 (§6.16.9)。結果は幅 32 の 2 値 (`integer` 型への格納は 4 値だが X/Z は出ない、
/// plan.md §3.5.1)。規則は [`atoi_u32`] を参照。
pub fn atoi(s: &[u8], radix: Radix) -> BitVec {
    BitVec::from_u64(32, u64::from(atoi_u32(s, radix)))
}

impl LogicVec {
    /// 文字列リテラル → 幅 `width` の整数 (§5.9, §11.10.3)。[`from_string_bytes`] を参照。
    pub fn from_string_bytes(width: u32, bytes: &[u8]) -> LogicVec {
        let mut r = LogicVec::zero(width);
        from_string_bytes(r.as_mut(), bytes);
        r
    }

    /// 整数 → `string` 型のバイト列 (§6.16)。[`to_string_bytes`] を参照 (X/Z は 0。[要VCS確認 V-5])。
    pub fn to_string_bytes(&self) -> Vec<u8> {
        to_string_bytes(self.as_ref())
    }
}

impl BitVec {
    /// 文字列リテラル → 幅 `width` の 2 値の整数 (§5.9, §11.10.3)。
    pub fn from_string_bytes(width: u32, bytes: &[u8]) -> BitVec {
        let mut r = BitVec::zero(width);
        from_string_bytes2(r.as_mut(), bytes);
        r
    }

    /// 整数 → `string` 型のバイト列 (§6.16)。
    pub fn to_string_bytes(&self) -> Vec<u8> {
        to_string_bytes2(self.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn s11_10_3_from_string_pads_and_truncates() {
        assert_eq!(
            LogicVec::from_string_bytes(16, b"AB").to_u64(),
            Some(0x4142)
        );
        assert_eq!(LogicVec::from_string_bytes(12, b"AB").to_u64(), Some(0x142));
        assert_eq!(LogicVec::from_string_bytes(8, b"").to_u64(), Some(0));
        let v = LogicVec::from_string_bytes(72, b"ABCDEFGHI");
        assert_eq!(v.to_string_bytes(), b"ABCDEFGHI");
    }

    #[test]
    fn s6_16_to_string_xz_as_zero_v5() {
        // [要VCS確認 V-5]
        let v = LogicVec::from_bits_str("01000001xxxxxxxx").unwrap();
        assert_eq!(v.to_string_bytes(), b"A");
    }

    #[test]
    fn s6_16_9_atoi_basic() {
        assert_eq!(atoi_u32(b"12_3", Radix::Dec), 123);
        assert_eq!(atoi_u32(b"-12", Radix::Dec), 0);
        assert_eq!(atoi_u32(b"4294967297", Radix::Dec), 1);
        assert_eq!(atoi_u32(b"fF", Radix::Hex), 255);
        assert_eq!(atoi(b"17", Radix::Oct).to_u64(), Some(15));
    }
}
