// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

//! Int8 GEMM, normalisation, activations and pooling. SIMD paths are runtime
//! dispatched; the scalar path is the differential-test oracle.

use bk_core::SimdFeatures;

/// Scalar dot product of two i8 slices (matching lengths are truncated to the
/// shorter one).
#[inline]
pub fn dot_i8_scalar(a: &[i8], b: &[i8]) -> i32 {
    let n = a.len().min(b.len());
    let mut acc = 0i32;
    let mut i = 0;
    // Four independent accumulators improve ILP.
    let mut a0 = 0i32;
    let mut a1 = 0i32;
    let mut a2 = 0i32;
    let mut a3 = 0i32;
    while i + 4 <= n {
        a0 += a[i] as i32 * b[i] as i32;
        a1 += a[i + 1] as i32 * b[i + 1] as i32;
        a2 += a[i + 2] as i32 * b[i + 2] as i32;
        a3 += a[i + 3] as i32 * b[i + 3] as i32;
        i += 4;
    }
    acc += a0 + a1 + a2 + a3;
    while i < n {
        acc += a[i] as i32 * b[i] as i32;
        i += 1;
    }
    acc
}

/// Dispatched dot product.
#[inline]
pub fn dot_i8(a: &[i8], b: &[i8]) -> i32 {
    #[cfg(target_arch = "x86_64")]
    {
        let feats = SimdFeatures::detect();
        if feats.avx2 {
            // SAFETY: avx2 was detected at runtime.
            return unsafe { dot_avx2(a, b) };
        }
    }
    dot_i8_scalar(a, b)
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx2")]
unsafe fn dot_avx2(a: &[i8], b: &[i8]) -> i32 {
    use core::arch::x86_64::*;
    let n = a.len().min(b.len());
    let mut i = 0usize;
    let mut acc = _mm256_setzero_si256();
    unsafe {
        while i + 16 <= n {
            let av = _mm_loadu_si128(a.as_ptr().add(i) as *const __m128i);
            let bv = _mm_loadu_si128(b.as_ptr().add(i) as *const __m128i);
            let a16 = _mm256_cvtepi8_epi16(av);
            let b16 = _mm256_cvtepi8_epi16(bv);
            acc = _mm256_add_epi32(acc, _mm256_madd_epi16(a16, b16));
            i += 16;
        }
        let mut tmp = [0i32; 8];
        _mm256_storeu_si256(tmp.as_mut_ptr() as *mut __m256i, acc);
        let mut sum: i32 = tmp.iter().sum();
        while i < n {
            sum += *a.get_unchecked(i) as i32 * *b.get_unchecked(i) as i32;
            i += 1;
        }
        sum
    }
}

/// C[m,n] = A[m,k] · W[n,k]ᵀ. `w` is row-major `[n, k]`, which is the natural
/// layout for a linear layer's weight matrix.
pub fn int8_gemm(a: &[i8], w: &[i8], m: usize, n: usize, k: usize, out: &mut [i32]) {
    debug_assert!(a.len() >= m * k);
    debug_assert!(w.len() >= n * k);
    debug_assert!(out.len() >= m * n);
    for i in 0..m {
        let arow = &a[i * k..(i + 1) * k];
        for j in 0..n {
            out[i * n + j] = dot_i8(arow, &w[j * k..(j + 1) * k]);
        }
    }
}

/// LayerNorm over the last dimension.
pub fn layer_norm(x: &[f32], gamma: &[f32], beta: &[f32], eps: f32, out: &mut [f32]) {
    let n = x.len();
    if n == 0 {
        return;
    }
    let mean = x.iter().sum::<f32>() / n as f32;
    let var = x.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n as f32;
    let inv = 1.0 / (var + eps).sqrt();
    for i in 0..n {
        let g = gamma.get(i).copied().unwrap_or(1.0);
        let b = beta.get(i).copied().unwrap_or(0.0);
        out[i] = (x[i] - mean) * inv * g + b;
    }
}

/// RMSNorm.
pub fn rms_norm(x: &[f32], gamma: &[f32], eps: f32, out: &mut [f32]) {
    let n = x.len();
    if n == 0 {
        return;
    }
    let ms = x.iter().map(|v| v * v).sum::<f32>() / n as f32;
    let inv = 1.0 / (ms + eps).sqrt();
    for i in 0..n {
        let g = gamma.get(i).copied().unwrap_or(1.0);
        out[i] = x[i] * inv * g;
    }
}

/// GELU (tanh approximation).
pub fn gelu(x: &[f32], out: &mut [f32]) {
    const C: f32 = 0.797_884_6;
    for (i, &v) in x.iter().enumerate() {
        let inner = C * (v + 0.044_715 * v * v * v);
        out[i] = 0.5 * v * (1.0 + inner.tanh());
    }
}

/// Numerically stable softmax.
pub fn softmax(x: &[f32], out: &mut [f32]) {
    let max = x.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let mut sum = 0.0f32;
    for (i, &v) in x.iter().enumerate() {
        let e = (v - max).exp();
        out[i] = e;
        sum += e;
    }
    if sum > 0.0 {
        for o in out.iter_mut() {
            *o /= sum;
        }
    }
}

/// 1-D max pool with stride.
pub fn maxpool1d(x: &[f32], k: usize, stride: usize, out: &mut [f32]) {
    if k == 0 || x.is_empty() {
        return;
    }
    let mut o = 0;
    let mut i = 0;
    while i + k <= x.len() {
        let mut m = f32::NEG_INFINITY;
        for v in &x[i..i + k] {
            m = m.max(*v);
        }
        if o < out.len() {
            out[o] = m;
        }
        o += 1;
        i += stride.max(1);
    }
}

/// Int8 conv1d over `in_ch` input channels producing `out_ch` channels.
/// `x` is `[in_ch, len]`, `w` is `[out_ch, in_ch, kernel]`.
#[allow(clippy::too_many_arguments)]
pub fn conv1d_int8(
    x: &[i8],
    w: &[i8],
    b: &[i32],
    in_ch: usize,
    out_ch: usize,
    len: usize,
    kernel: usize,
    stride: usize,
    out: &mut [i32],
) {
    if kernel == 0 || stride == 0 {
        return;
    }
    let out_len = if len >= kernel {
        (len - kernel) / stride + 1
    } else {
        0
    };
    for oc in 0..out_ch {
        for pos in 0..out_len {
            let mut acc = b.get(oc).copied().unwrap_or(0);
            for ic in 0..in_ch {
                let xrow = &x[ic * len..(ic + 1) * len];
                let wrow = &w[(oc * in_ch + ic) * kernel..(oc * in_ch + ic + 1) * kernel];
                let start = pos * stride;
                for kw in 0..kernel {
                    acc += xrow[start + kw] as i32 * wrow[kw] as i32;
                }
            }
            out[oc * out_len + pos] = acc;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gemm_matches_reference() {
        let a: Vec<i8> = (0..12).map(|i| (i - 6) as i8).collect(); // 3x4
        let w: Vec<i8> = (0..8).map(|i| (i - 4) as i8).collect(); // 2x4
        let mut out = vec![0i32; 3 * 2];
        int8_gemm(&a, &w, 3, 2, 4, &mut out);
        // Reference.
        for i in 0..3 {
            for j in 0..2 {
                let mut s = 0i32;
                for k in 0..4 {
                    s += a[i * 4 + k] as i32 * w[j * 4 + k] as i32;
                }
                assert_eq!(out[i * 2 + j], s);
            }
        }
    }

    #[test]
    fn dot_simd_matches_scalar() {
        for len in [0usize, 1, 7, 16, 31, 64, 100] {
            let a: Vec<i8> = (0..len).map(|i| ((i * 7) % 61) as i8 - 30).collect();
            let b: Vec<i8> = (0..len).map(|i| ((i * 13) % 53) as i8 - 26).collect();
            assert_eq!(dot_i8(&a, &b), dot_i8_scalar(&a, &b), "len={len}");
        }
    }

    #[test]
    fn softmax_sums_to_one() {
        let x = [1.0, 2.0, 3.0];
        let mut o = [0.0; 3];
        softmax(&x, &mut o);
        let s: f32 = o.iter().sum();
        assert!((s - 1.0).abs() < 1e-5);
    }

    #[test]
    fn maxpool() {
        let x = [1.0, 3.0, 2.0, 5.0, 4.0, 0.0];
        let mut o = [0.0; 3];
        maxpool1d(&x, 2, 2, &mut o);
        assert_eq!(o, [3.0, 5.0, 4.0]);
    }
}
