//! DPI の `svLogicVecVal` / `svBitVecVal` (IEEE 1800-2023 Annex I, `svdpi.h`) との変換 (plan.md §3.8)。
//!
//! # 配置
//!
//! - `svLogicVecVal` は `{ uint32_t aval; uint32_t bval; }` の配列で、32 ビットごとに aval と bval が
//!   交互に並ぶ。`u32` の列にすると `[aval_0, bval_0, aval_1, bval_1, …]` で、要素 (組) の数は
//!   `(w + 31) >> 5` (Annex I)。符号化は 0=(0,0)、1=(1,0)、z=(0,1)、x=(1,1) で、内部表現と同じ
//!   (plan.md §3.1)。
//! - `svBitVecVal` は `uint32_t` の配列で、要素数は `(w + 31) >> 5`。
//! - 内部表現は 64 ビットのプレーン分離 (設計判断 D-1、plan.md §3.2) なので、4 値は常に詰め替える。
//!
//! # ゼロコピーにできる場合
//!
//! 2 値の配置 `[a_0, …]` (u64、リトルエンディアン) は、リトルエンディアンの `u32` 配列と同じバイト列に
//! なる。そのため **2 値の input 引数 (const ポインタ) に限り** アリーナ上のワード列をそのまま渡せる。
//! `u32` の要素数 `ceil(w/32)` は `u64` のワード数の 2 倍以下なので、境界をはみ出さない。
//!
//! output / inout 引数は、2 値でも 4 値でも、一時バッファに受け取り、`read_*` で最上位要素の
//! 未使用ビットをマスクしてから実引数へ代入する (R-20)。C 側が未使用ビットに何を書いても、
//! 読み込み時にマスクして正規化不変条件 (plan.md §3.2) を守る。
//!
//! # 誤用
//!
//! 配列の長さが [`sv_logic_vec_val_len`] / [`sv_bit_vec_val_len`] と違う場合は panic する
//! (release ビルドでも。API の誤用は panic、plan.md §3.3 規約)。

use crate::view::{BitMut, BitRef, LogicMut, LogicRef};

// ゼロコピーの前提 (u64 と u32 のバイト列が一致する) はリトルエンディアンでしか成り立たない (R-13)。
#[cfg(not(target_endian = "little"))]
compile_error!("svsim-logic requires a little-endian target (plan.md §3.2, svBitVecVal zero-copy)");

/// 幅 `width` の `svBitVecVal` 配列の要素数 `(width + 31) >> 5` (Annex I)。
#[inline]
pub const fn sv_bit_vec_val_len(width: u32) -> usize {
    (width as usize).div_ceil(32)
}

/// 幅 `width` の `svLogicVecVal` 配列を `u32` の列として見たときの長さ
/// (`2 * ((width + 31) >> 5)`。aval と bval が交互に並ぶ)。
#[inline]
pub const fn sv_logic_vec_val_len(width: u32) -> usize {
    2 * sv_bit_vec_val_len(width)
}

/// プレーン `plane` (u64 のワード列) の、32 ビット単位の `k` 番目の要素。
#[inline]
fn get32(plane: &[u64], k: usize) -> u32 {
    (plane[k / 2] >> (32 * (k % 2))) as u32
}

/// プレーン `plane` の 32 ビット単位の `k` 番目の要素に `v` を書く (その位置は 0 であること)。
#[inline]
fn or32(plane: &mut [u64], k: usize, v: u32) {
    plane[k / 2] |= (v as u64) << (32 * (k % 2));
}

/// `LogicRef` → `svLogicVecVal` 配列 (u32 の aval/bval が交互に並ぶ。長さは [`sv_logic_vec_val_len`])。
///
/// `src` は正規化されているので、最上位要素の未使用ビットは 0 になる。
///
/// # Panics
/// `dst.len() != sv_logic_vec_val_len(src.width())` の場合。
pub fn write_sv_logic_vec_val(src: LogicRef<'_>, dst: &mut [u32]) {
    let n = sv_bit_vec_val_len(src.width());
    assert_eq!(
        dst.len(),
        2 * n,
        "write_sv_logic_vec_val: dst length (u32 count) must be {} for width {}, got {}",
        2 * n,
        src.width(),
        dst.len()
    );
    let (a, b) = (src.aval(), src.bval());
    for (k, pair) in dst.chunks_exact_mut(2).enumerate() {
        pair[0] = get32(a, k);
        pair[1] = get32(b, k);
    }
}

/// `svLogicVecVal` 配列 → `LogicMut`。最上位要素の `w` ビット目以上は読み捨ててマスクする。
///
/// # Panics
/// `src.len() != sv_logic_vec_val_len(dst.width())` の場合。
pub fn read_sv_logic_vec_val(src: &[u32], mut dst: LogicMut<'_>) {
    let width = dst.width();
    let n = sv_bit_vec_val_len(width);
    assert_eq!(
        src.len(),
        2 * n,
        "read_sv_logic_vec_val: src length (u32 count) must be {} for width {}, got {}",
        2 * n,
        width,
        src.len()
    );
    {
        let (a, b) = dst.planes_mut();
        a.fill(0);
        b.fill(0);
        for (k, pair) in src.chunks_exact(2).enumerate() {
            or32(a, k, pair[0]);
            or32(b, k, pair[1]);
        }
    }
    dst.normalize();
}

/// `BitRef` → `svBitVecVal` 配列 (長さは [`sv_bit_vec_val_len`])。
///
/// # Panics
/// `dst.len() != sv_bit_vec_val_len(src.width())` の場合。
pub fn write_sv_bit_vec_val(src: BitRef<'_>, dst: &mut [u32]) {
    let n = sv_bit_vec_val_len(src.width());
    assert_eq!(
        dst.len(),
        n,
        "write_sv_bit_vec_val: dst length (u32 count) must be {} for width {}, got {}",
        n,
        src.width(),
        dst.len()
    );
    let a = src.words();
    for (k, d) in dst.iter_mut().enumerate() {
        *d = get32(a, k);
    }
}

/// `svBitVecVal` 配列 → `BitMut`。最上位要素の `w` ビット目以上は読み捨ててマスクする。
///
/// # Panics
/// `src.len() != sv_bit_vec_val_len(dst.width())` の場合。
pub fn read_sv_bit_vec_val(src: &[u32], mut dst: BitMut<'_>) {
    let width = dst.width();
    let n = sv_bit_vec_val_len(width);
    assert_eq!(
        src.len(),
        n,
        "read_sv_bit_vec_val: src length (u32 count) must be {} for width {}, got {}",
        n,
        width,
        src.len()
    );
    {
        let a = dst.words_mut();
        a.fill(0);
        for (k, &v) in src.iter().enumerate() {
            or32(a, k, v);
        }
    }
    dst.normalize();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{BitVec, Logic, LogicVec};

    #[test]
    fn lengths() {
        // Annex I: 要素数は (w + 31) >> 5。
        for (w, n) in [
            (0u32, 0usize),
            (1, 1),
            (31, 1),
            (32, 1),
            (33, 2),
            (63, 2),
            (64, 2),
            (65, 3),
            (127, 4),
            (128, 4),
            (129, 5),
        ] {
            assert_eq!(sv_bit_vec_val_len(w), n, "width {w}");
            assert_eq!(sv_logic_vec_val_len(w), 2 * n, "width {w}");
            // u32 の要素数は u64 のワード数の 2 倍以下 (ゼロコピーで境界をはみ出さない、plan.md §3.8)。
            assert!(n <= 2 * crate::words_for(w));
        }
    }

    #[test]
    fn logic_round_trip_small() {
        let v = LogicVec::from_bits_str("x0z10000").unwrap();
        let mut buf = vec![0u32; sv_logic_vec_val_len(v.width())];
        write_sv_logic_vec_val(v.as_ref(), &mut buf);
        // x0z10000: aval = 1001_0000 = 0x90、bval = 1010_0000 = 0xa0
        assert_eq!(buf, [0x90, 0xa0]);
        let mut back = LogicVec::zero(v.width());
        read_sv_logic_vec_val(&buf, back.as_mut());
        assert_eq!(back, v);
    }

    #[test]
    fn read_masks_unused_bits() {
        // 幅 1、未使用ビットに 1 → z (a=0, b=1) だけが残る。
        let mut v = LogicVec::filled(1, Logic::L1);
        read_sv_logic_vec_val(&[0xffff_fffe, 0xffff_ffff], v.as_mut());
        assert_eq!(v.to_bits_string(), "z");
        assert!(v.as_ref().is_normalized());

        let mut b = BitVec::zero(33);
        read_sv_bit_vec_val(&[0x1234_5678, 0xffff_ffff], b.as_mut());
        assert_eq!(b, BitVec::from_u64(33, 0x1_1234_5678));
        assert!(b.as_ref().is_normalized());
    }

    #[test]
    #[should_panic(expected = "write_sv_logic_vec_val: dst length")]
    fn write_rejects_bad_length() {
        let v = LogicVec::zero(33);
        let mut buf = [0u32; 2];
        write_sv_logic_vec_val(v.as_ref(), &mut buf);
    }

    #[test]
    #[should_panic(expected = "read_sv_bit_vec_val: src length")]
    fn read_rejects_bad_length() {
        let mut v = BitVec::zero(33);
        read_sv_bit_vec_val(&[0], v.as_mut());
    }

    #[test]
    fn width_zero() {
        let v = LogicVec::zero(0);
        write_sv_logic_vec_val(v.as_ref(), &mut []);
        let mut w = LogicVec::zero(0);
        read_sv_logic_vec_val(&[], w.as_mut());
        let b = BitVec::zero(0);
        write_sv_bit_vec_val(b.as_ref(), &mut []);
        let mut c = BitVec::zero(0);
        read_sv_bit_vec_val(&[], c.as_mut());
    }
}
