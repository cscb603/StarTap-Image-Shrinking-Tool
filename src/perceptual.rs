//! v4.2.0-exp 感知压缩模块（perceptual-exp 分支）
//!
//! 组成：
//! - 双边滤波降噪（P1，纯 Rust，rayon 行并行）
//! - 频谱残差显著性检测（P1，64x64 频域，自带 radix-2 FFT，零依赖）
//! - 显著性掩码加权 USM 锐化（P1，复用 lib 的高斯模糊与肤色保护）
//! - 灰度 SSIM / PSNR（P1，验收指标）
//!
//! 铁律：本模块只被 CLI/AI 通路引用（ProcessConfig.perceptual = Some），
//! GUI 通路恒为 None，行为 100% 保持 v4.1.0。

use image::{DynamicImage, ImageBuffer, Rgba};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

// ============================================================================
// 参数结构
// ============================================================================

/// 锐化焦点模式
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum FocusMode {
    /// 频谱残差显著性检测（主体自动识别）
    Auto,
    /// 中心高斯权重（构图居中的照片）
    Center,
}

impl FocusMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            FocusMode::Auto => "auto",
            FocusMode::Center => "center",
        }
    }
}

/// 量化表模式（P2 接线）
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum QuantMode {
    /// 标准 Annex-K 表（v4.1.0 行为）
    Standard,
    /// jpeg-encoder 内置 MS-SSIM 调优表（P2-A）
    MsSsim,
    /// 自算 CSF 对比敏感度 + 纹理掩蔽表（P2-B）
    Csf,
}

impl QuantMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            QuantMode::Standard => "standard",
            QuantMode::MsSsim => "msssim",
            QuantMode::Csf => "csf",
        }
    }
}

/// 感知压缩选项（全部经 CLI/JSON 显式开启；GUI 恒为 None）
#[derive(Clone, Debug)]
pub struct PerceptualOptions {
    /// 降噪强度 0-100，0=关闭；JPG 输入强制跳过（二次压缩禁降噪）
    pub denoise_strength: u8,
    /// 锐化焦点
    pub focus_mode: FocusMode,
    /// 感知模式质量上限（防止堆质量爆体积）
    pub quality_ceil: u8,
    /// 量化表模式
    pub quant_mode: QuantMode,
    /// 体积预算 KB（覆盖 target_kb；None 时用 ProcessConfig.target_kb）
    pub budget_kb: Option<u32>,
    /// 平台预设名（仅记录用）
    pub platform: Option<String>,
}

impl Default for PerceptualOptions {
    fn default() -> Self {
        Self {
            denoise_strength: 25,
            focus_mode: FocusMode::Auto,
            quality_ceil: 95,
            quant_mode: QuantMode::Standard,
            budget_kb: None,
            platform: None,
        }
    }
}

/// 感知模式单张指标（可观测性，进 JSON metrics / benchmark）
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PerceptualMetrics {
    pub ssim_vs_source: f64,
    pub psnr_vs_source: f64,
    pub final_quality: u8,
    pub denoise_applied: bool,
    pub denoise_ms: u64,
    pub downscale_ms: u64,
    pub sharpen_ms: u64,
    pub encode_ms: u64,
}

// ============================================================================
// P1-1 双边滤波降噪（5x5 核，luma 距离做值域权重，rayon 行并行）
// ============================================================================

#[inline]
fn luma_f32(r: u8, g: u8, b: u8) -> f32 {
    0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32
}

/// 双边滤波：空间高斯 x 值域高斯，保边去噪。
/// strength 0-100 映射值域 sigma（25 → 约 12.75，保守宁欠勿过）。
pub fn bilateral_denoise(
    img: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    strength: u8,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    if strength == 0 {
        return img.clone();
    }
    let (w, h) = img.dimensions();
    if w < 5 || h < 5 {
        return img.clone();
    }

    const RADIUS: i32 = 2;
    let sigma_s = 1.7f32;
    let sigma_r = 4.0 + strength.min(100) as f32 * 0.35;
    let two_sr2 = 2.0 * sigma_r * sigma_r;

    // 预计算 5x5 空间权重
    let mut spatial = [[0f32; 5]; 5];
    for (dy, row) in spatial.iter_mut().enumerate() {
        for (dx, v) in row.iter_mut().enumerate() {
            let fy = dy as f32 - RADIUS as f32;
            let fx = dx as f32 - RADIUS as f32;
            *v = (-(fx * fx + fy * fy) / (2.0 * sigma_s * sigma_s)).exp();
        }
    }

    let src = img.as_raw();
    let row_stride = (w * 4) as usize;
    let mut out = vec![0u8; src.len()];

    out.par_chunks_mut(row_stride)
        .enumerate()
        .for_each(|(row_y, row)| {
            let y = row_y as i32;
            for x in 0..w as i32 {
                let ci = (row_y * row_stride) + (x as usize) * 4;
                let cl = luma_f32(src[ci], src[ci + 1], src[ci + 2]);
                let (mut acc_r, mut acc_g, mut acc_b, mut acc_w) = (0f32, 0f32, 0f32, 0f32);
                for dy in -RADIUS..=RADIUS {
                    let yy = (y + dy).clamp(0, h as i32 - 1) as usize;
                    for dx in -RADIUS..=RADIUS {
                        let xx = (x + dx).clamp(0, w as i32 - 1) as usize;
                        let ni = yy * row_stride + xx * 4;
                        let nl = luma_f32(src[ni], src[ni + 1], src[ni + 2]);
                        let dl = nl - cl;
                        let wgt = spatial[(dy + RADIUS) as usize][(dx + RADIUS) as usize]
                            * (-(dl * dl) / two_sr2).exp();
                        acc_r += src[ni] as f32 * wgt;
                        acc_g += src[ni + 1] as f32 * wgt;
                        acc_b += src[ni + 2] as f32 * wgt;
                        acc_w += wgt;
                    }
                }
                let o = (x as usize) * 4;
                if acc_w > 0.0 {
                    row[o] = (acc_r / acc_w).round().clamp(0.0, 255.0) as u8;
                    row[o + 1] = (acc_g / acc_w).round().clamp(0.0, 255.0) as u8;
                    row[o + 2] = (acc_b / acc_w).round().clamp(0.0, 255.0) as u8;
                } else {
                    row[o] = src[ci];
                    row[o + 1] = src[ci + 1];
                    row[o + 2] = src[ci + 2];
                }
                row[o + 3] = src[ci + 3];
            }
        });

    ImageBuffer::from_raw(w, h, out).expect("bilateral output buffer size mismatch")
}

// ============================================================================
// P1-2 频谱残差显著性（Spectral Residual, Hou & Zhang 2007）
// 64x64 频域计算，radix-2 FFT 手写，输出 0..1 全尺寸掩码
// ============================================================================

const SAL_SIZE: usize = 64;

/// radix-2 迭代 FFT（长度必须为 2 的幂）
fn fft1d(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    debug_assert!(n.is_power_of_two() && im.len() == n);

    // 位反转重排
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2usize;
    while len <= n {
        let ang = sign * 2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0usize;
        while i < n {
            let (mut cur_r, mut cur_i) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (ur, ui) = (re[i + k], im[i + k]);
                let (vr0, vi0) = (re[i + k + len / 2], im[i + k + len / 2]);
                let vr = vr0 * cur_r - vi0 * cur_i;
                let vi = vr0 * cur_i + vi0 * cur_r;
                re[i + k] = ur + vr;
                im[i + k] = ui + vi;
                re[i + k + len / 2] = ur - vr;
                im[i + k + len / 2] = ui - vi;
                let nr = cur_r * wr - cur_i * wi;
                cur_i = cur_r * wi + cur_i * wr;
                cur_r = nr;
            }
            i += len;
        }
        len <<= 1;
    }

    if inverse {
        let inv_n = 1.0 / n as f64;
        for v in re.iter_mut() {
            *v *= inv_n;
        }
        for v in im.iter_mut() {
            *v *= inv_n;
        }
    }
}

/// 2D FFT（先行后列）
fn fft2d(re: &mut [f64], im: &mut [f64], n: usize, inverse: bool) {
    // 行
    for y in 0..n {
        fft1d(
            &mut re[y * n..(y + 1) * n],
            &mut im[y * n..(y + 1) * n],
            inverse,
        );
    }
    // 列（转置缓冲）
    let mut col_r = vec![0f64; n];
    let mut col_i = vec![0f64; n];
    for x in 0..n {
        for y in 0..n {
            col_r[y] = re[y * n + x];
            col_i[y] = im[y * n + x];
        }
        fft1d(&mut col_r, &mut col_i, inverse);
        for y in 0..n {
            re[y * n + x] = col_r[y];
            im[y * n + x] = col_i[y];
        }
    }
}

/// 频谱残差显著性掩码：返回长度 w*h 的 0..1 f32 数组（主体亮、背景暗）
pub fn saliency_mask(img: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Vec<f32> {
    let (w, h) = img.dimensions();
    let n = SAL_SIZE;

    // 1) box 降采样到 64x64 灰度
    let mut small = vec![0f64; n * n];
    let src = img.as_raw();
    let row_stride = (w * 4) as usize;
    for sy in 0..n {
        let y0 = sy as u32 * h / n as u32;
        let y1 = ((sy as u32 + 1) * h / n as u32).max(y0 + 1).min(h);
        for sx in 0..n {
            let x0 = sx as u32 * w / n as u32;
            let x1 = ((sx as u32 + 1) * w / n as u32).max(x0 + 1).min(w);
            let mut acc = 0f64;
            let mut cnt = 0f64;
            for y in y0..y1 {
                let base = y as usize * row_stride;
                for x in x0..x1 {
                    let i = base + x as usize * 4;
                    acc += luma_f32(src[i], src[i + 1], src[i + 2]) as f64;
                    cnt += 1.0;
                }
            }
            small[sy * n + sx] = if cnt > 0.0 { acc / cnt } else { 0.0 };
        }
    }

    // 2) FFT → 对数幅度谱 → 3x3 均值平滑 → 频谱残差 → 逆 FFT
    let mut re = small.clone();
    let mut im = vec![0f64; n * n];
    fft2d(&mut re, &mut im, n, false);

    let mut log_amp = vec![0f64; n * n];
    let mut phase = vec![0f64; n * n];
    for i in 0..n * n {
        let amp = (re[i] * re[i] + im[i] * im[i]).sqrt();
        log_amp[i] = (amp + 1e-8).ln();
        phase[i] = im[i].atan2(re[i]);
    }

    // 3x3 均值平滑对数幅度谱
    let mut smooth = vec![0f64; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut acc = 0f64;
            let mut cnt = 0f64;
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    let yy = (y as i32 + dy).clamp(0, n as i32 - 1) as usize;
                    let xx = (x as i32 + dx).clamp(0, n as i32 - 1) as usize;
                    acc += log_amp[yy * n + xx];
                    cnt += 1.0;
                }
            }
            smooth[y * n + x] = acc / cnt;
        }
    }

    for i in 0..n * n {
        let residual = log_amp[i] - smooth[i];
        let amp = residual.exp();
        re[i] = amp * phase[i].cos();
        im[i] = amp * phase[i].sin();
    }
    fft2d(&mut re, &mut im, n, true);

    // 3) 幅度平方 → 高斯平滑（σ≈2.5）→ 归一化 0..1
    let mut sal = vec![0f32; n * n];
    for i in 0..n * n {
        sal[i] = (re[i] * re[i] + im[i] * im[i]) as f32;
    }
    let sal = gaussian_smooth_small(&sal, n, 2.5);

    let max_v = sal.iter().cloned().fold(f32::MIN, f32::max);
    let min_v = sal.iter().cloned().fold(f32::MAX, f32::min);
    let range = (max_v - min_v).max(1e-9);
    let norm: Vec<f32> = sal.iter().map(|v| (v - min_v) / range).collect();

    // 4) 双线性上采样回全尺寸
    upscale_bilinear(&norm, n, n, w as usize, h as usize)
}

/// 小尺寸图的可分离高斯平滑
fn gaussian_smooth_small(data: &[f32], n: usize, sigma: f32) -> Vec<f32> {
    let ksize = ((sigma * 6.0) as usize).max(3) | 1;
    let half = (ksize / 2) as i32;
    let mut kernel = vec![0f32; ksize];
    let two_s2 = 2.0 * sigma * sigma;
    let mut sum = 0f32;
    for (i, v) in kernel.iter_mut().enumerate() {
        let d = i as f32 - half as f32;
        *v = (-d * d / two_s2).exp();
        sum += *v;
    }
    for v in kernel.iter_mut() {
        *v /= sum;
    }

    let mut tmp = vec![0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut acc = 0f32;
            for (k, &kw) in kernel.iter().enumerate() {
                let xx = (x as i32 + k as i32 - half).clamp(0, n as i32 - 1) as usize;
                acc += data[y * n + xx] * kw;
            }
            tmp[y * n + x] = acc;
        }
    }
    let mut out = vec![0f32; n * n];
    for y in 0..n {
        for x in 0..n {
            let mut acc = 0f32;
            for (k, &kw) in kernel.iter().enumerate() {
                let yy = (y as i32 + k as i32 - half).clamp(0, n as i32 - 1) as usize;
                acc += tmp[yy * n + x] * kw;
            }
            out[y * n + x] = acc;
        }
    }
    out
}

/// 双线性上采样 f32 掩码
fn upscale_bilinear(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let mut out = vec![0f32; dw * dh];
    let sx_ratio = sw as f32 / dw as f32;
    let sy_ratio = sh as f32 / dh as f32;
    for y in 0..dh {
        let fy = (y as f32 + 0.5) * sy_ratio - 0.5;
        let y0 = fy.floor().clamp(0.0, sh as f32 - 1.0) as usize;
        let y1 = (y0 + 1).min(sh - 1);
        let ty = (fy - y0 as f32).clamp(0.0, 1.0);
        for x in 0..dw {
            let fx = (x as f32 + 0.5) * sx_ratio - 0.5;
            let x0 = fx.floor().clamp(0.0, sw as f32 - 1.0) as usize;
            let x1 = (x0 + 1).min(sw - 1);
            let tx = (fx - x0 as f32).clamp(0.0, 1.0);
            let top = src[y0 * sw + x0] * (1.0 - tx) + src[y0 * sw + x1] * tx;
            let bot = src[y1 * sw + x0] * (1.0 - tx) + src[y1 * sw + x1] * tx;
            out[y * dw + x] = top * (1.0 - ty) + bot * ty;
        }
    }
    out
}

/// 中心高斯权重掩码（focus-mode=center）
pub fn center_mask(w: u32, h: u32) -> Vec<f32> {
    let (wf, hf) = (w as f32, h as f32);
    let (cx, cy) = (wf / 2.0, hf / 2.0);
    let sigma = 0.45 * wf.min(hf);
    let two_s2 = 2.0 * sigma * sigma;
    let mut out = vec![0f32; (w * h) as usize];
    for y in 0..h {
        let dy = y as f32 - cy;
        for x in 0..w {
            let dx = x as f32 - cx;
            out[(y * w + x) as usize] = (-(dx * dx + dy * dy) / two_s2).exp();
        }
    }
    out
}

// ============================================================================
// P1-3 显著性掩码加权 USM 锐化（主体 100%，平滑背景趋近 0%）
// ============================================================================

/// 掩码加权 USM：out = orig + amount * mask * skin_factor * (orig - blurred)
/// 掩码已在 64x64 低分辨率高斯平滑 + 双线性上采样，天然无锯齿边界。
pub fn masked_usm_sharpen(
    image: &DynamicImage,
    mask: &[f32],
    radius: f32,
    amount: f32,
    threshold: u8,
) -> DynamicImage {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    debug_assert_eq!(mask.len(), (width * height) as usize);

    let blurred = crate::gaussian_blur(&rgba, radius);
    let is_portrait = crate::estimate_skin_ratio(image) > 0.3;

    let mut output = ImageBuffer::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let orig = rgba.get_pixel(x, y);
            let blur = blurred.get_pixel(x, y);
            let m = mask[(y * width + x) as usize].clamp(0.0, 1.0);

            let skin_score = crate::is_skin_color(orig[0], orig[1], orig[2]);
            let skin_factor = if is_portrait && skin_score > 0.3 {
                1.0 - skin_score * 0.8
            } else {
                1.0
            };

            let effective = amount * m * skin_factor;
            let mut out_px = Rgba([0u8; 4]);
            for c in 0..3 {
                let diff = orig[c] as i16 - blur[c] as i16;
                if diff.abs() > threshold as i16 && effective > 0.01 {
                    let sharpened = orig[c] as i32 + (effective * diff as f32) as i32;
                    out_px[c] = sharpened.clamp(0, 255) as u8;
                } else {
                    out_px[c] = orig[c];
                }
            }
            out_px[3] = orig[3];
            output.put_pixel(x, y, out_px);
        }
    }
    DynamicImage::ImageRgba8(output)
}

// ============================================================================
// P2-B 自算 CSF 感知量化表（Mannos-Sakrison 对比敏感度函数）
// ============================================================================
//
// ⚠️ 坑（蓝图 §11.2-6，jpeg-encoder quantization.rs:222 实证）：
// QuantizationTableType::Custom 表按原值直用、不做 quality 缩放！
// 所以本函数必须自己套用 libjpeg 缩放公式：
//   scale = q<50 ? 5000/q : 200-2q;  v' = clamp((v*scale+50)/100, 1, 255)
//
// 设计：
// - 空间频率 f(u,v) = sqrt(u²+v²) * fs/16（fs≈32 cyc/deg，普通观看距离）
// - CSF(f) = 2.6·(0.0192+0.114f)·exp(-(0.114f)^1.1)（Mannos & Sakrison 1974）
// - 量化步长 ∝ 1/CSF：人眼敏感的中低频少砍，不敏感的高频多砍
// - CSF 感知量化表：以「标准 JPEG 量化表」为基底，用 Mannos-Sakrison CSF 敏感度做逐系数调制。
//   可见频带（CSF 高于均值）→ 更细量化（多给码率）；不可见频带（高频/色度高频）→ 更粗量化（让码率）。
//   调制比用「均值归一化」锚定到平均≈1，整体码率预算与同 quality 标准表持平（避免朴素 CSF 表体积暴涨）。

/// JPEG Annex-K 标准亮度量化表基底（quality=50 基准）
const STD_LUMA_BASE: [u16; 64] = [
    16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56,
    14, 17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113,
    92, 49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
];
/// JPEG Annex-K 标准色度量化表基底（quality=50 基准）
const STD_CHROMA_BASE: [u16; 64] = [
    17, 18, 24, 47, 99, 99, 99, 99, 18, 21, 26, 66, 99, 99, 99, 99, 24, 26, 56, 99, 99, 99, 99, 99,
    47, 66, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
    99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99, 99,
];

/// 生成按 quality 缩放后的 CSF 感知量化表 (luma, chroma)
///
/// 实现方式：先按 libjpeg 公式生成同 quality 的标准表，再用「均值归一化」的 CSF 敏感度逐系数调制
/// （`csf[i]` = 标准表 `[i]` / (sens`[i]`/mean_sens)）。平均调制比≈1 → 总码率与标准表持平；
/// 人眼更敏感的频带被加细（码率↑），更不敏感的频带被加粗（码率↓）→ 感知画质↑、体积不涨。
pub fn csf_quant_tables(quality: u8) -> ([u16; 64], [u16; 64]) {
    // Mannos-Sakrison CSF（原始形式，未归一化）
    let csf = |f: f64| -> f64 {
        if f < 1e-6 {
            1.0
        } else {
            2.6 * (0.0192 + 0.114 * f) * (-(0.114 * f).powf(1.1)).exp()
        }
    };

    const FS: f64 = 32.0; // 采样频率（cycles/degree），普通屏幕观看距离
    let freq = |u: usize, v: usize| -> f64 {
        let fu = u as f64 * FS / 16.0;
        let fv = v as f64 * FS / 16.0;
        (fu * fu + fv * fv).sqrt()
    };

    // 峰值 CSF 用于归一化（约在 f≈8 cyc/deg）
    let mut csf_peak = 0f64;
    for i in 0..80 {
        csf_peak = csf_peak.max(csf(i as f64 * 0.5));
    }

    // libjpeg quality 缩放公式（Custom 表不被 jpeg-encoder 缩放，必须自己做）
    let q = quality.clamp(1, 100) as u32;
    let scale = if q < 50 { 5000 / q } else { 200 - q * 2 };
    let std_table = |base: &[u16; 64]| -> [u16; 64] {
        let mut out = [0u16; 64];
        for (o, &b) in out.iter_mut().zip(base.iter()) {
            let v = (b as f64 * scale as f64 + 50.0) / 100.0;
            *o = (v.round() as u16).clamp(1, 255);
        }
        out
    };
    let std_luma = std_table(&STD_LUMA_BASE);
    let std_chroma = std_table(&STD_CHROMA_BASE);

    // 第一遍：算每个 AC 系数的归一化敏感度（色度指数更陡），并求均值用于预算平衡
    let mut luma_sens = [0f64; 64];
    let mut chroma_sens = [0f64; 64];
    let mut luma_sum = 0f64;
    let mut chroma_sum = 0f64;
    for v in 0..8 {
        for u in 0..8 {
            let i = v * 8 + u;
            let f = freq(u, v);
            let s = csf(f) / csf_peak;
            luma_sens[i] = s.clamp(0.05, 1.0);
            chroma_sens[i] = s.powf(1.35).clamp(0.05, 1.0);
            if i != 0 {
                luma_sum += luma_sens[i];
                chroma_sum += chroma_sens[i];
            }
        }
    }
    let luma_mean = luma_sum / 63.0;
    let chroma_mean = chroma_sum / 63.0;

    // 第二遍：调制（DC 不调制，防块状伪影）。m = sens/mean：可见频带 m>1→更细，不可见 m<1→更粗
    let build = |std: &[u16; 64], sens: &[f64; 64], mean: f64| -> [u16; 64] {
        let mut out = [0u16; 64];
        for v in 0..8 {
            for u in 0..8 {
                let i = v * 8 + u;
                if i == 0 {
                    out[i] = std[0];
                    continue;
                }
                let m = (sens[i] / mean).clamp(0.3, 3.0);
                let val = std[i] as f64 / m;
                out[i] = (val.round() as u16).clamp(1, 255);
            }
        }
        out
    };

    (
        build(&std_luma, &luma_sens, luma_mean),
        build(&std_chroma, &chroma_sens, chroma_mean),
    )
}

// ============================================================================
// P1-4 灰度 SSIM / PSNR（验收指标，纯 Rust 手写）
// ============================================================================

/// RGB 图转灰度 u8 数组
pub fn to_gray(img: &DynamicImage) -> (Vec<u8>, usize, usize) {
    let g = img.to_luma8();
    let (w, h) = g.dimensions();
    (g.into_raw(), w as usize, h as usize)
}

/// 灰度 SSIM，8x8 窗口不重叠，标准常数 C1/C2（K1=0.01, K2=0.03, L=255）
pub fn ssim_gray(a: &[u8], b: &[u8], w: usize, h: usize) -> f64 {
    if a.len() != w * h || b.len() != w * h || w < 8 || h < 8 {
        return 0.0;
    }
    const C1: f64 = 6.5025; // (0.01*255)^2
    const C2: f64 = 58.5225; // (0.03*255)^2
    const WIN: usize = 8;

    let mut total = 0f64;
    let mut count = 0usize;
    let mut by = 0;
    while by + WIN <= h {
        let mut bx = 0;
        while bx + WIN <= w {
            let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0f64, 0f64, 0f64, 0f64, 0f64);
            for y in by..by + WIN {
                for x in bx..bx + WIN {
                    let va = a[y * w + x] as f64;
                    let vb = b[y * w + x] as f64;
                    sa += va;
                    sb += vb;
                    saa += va * va;
                    sbb += vb * vb;
                    sab += va * vb;
                }
            }
            let np = (WIN * WIN) as f64;
            let mu_a = sa / np;
            let mu_b = sb / np;
            let var_a = saa / np - mu_a * mu_a;
            let var_b = sbb / np - mu_b * mu_b;
            let cov = sab / np - mu_a * mu_b;
            let ssim = ((2.0 * mu_a * mu_b + C1) * (2.0 * cov + C2))
                / ((mu_a * mu_a + mu_b * mu_b + C1) * (var_a + var_b + C2));
            total += ssim;
            count += 1;
            bx += WIN;
        }
        by += WIN;
    }
    if count == 0 {
        0.0
    } else {
        total / count as f64
    }
}

/// 灰度 PSNR（dB）
pub fn psnr_gray(a: &[u8], b: &[u8]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mse: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| {
            let d = x as f64 - y as f64;
            d * d
        })
        .sum::<f64>()
        / a.len() as f64;
    if mse <= 1e-12 {
        return 99.0;
    }
    10.0 * (255.0f64 * 255.0 / mse).log10()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fft_roundtrip() {
        let n = 8;
        let mut re: Vec<f64> = (0..n).map(|i| i as f64).collect();
        let mut im = vec![0f64; n];
        let orig = re.clone();
        fft1d(&mut re, &mut im, false);
        fft1d(&mut re, &mut im, true);
        for i in 0..n {
            assert!((re[i] - orig[i]).abs() < 1e-9);
        }
    }

    #[test]
    fn ssim_identical_is_one() {
        let w = 16;
        let h = 16;
        let a: Vec<u8> = (0..w * h).map(|i| (i % 251) as u8).collect();
        let s = ssim_gray(&a, &a, w, h);
        assert!((s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn psnr_identical_is_high() {
        let a = vec![128u8; 64];
        assert!(psnr_gray(&a, &a) > 90.0);
    }

    #[test]
    fn bilateral_keeps_dims() {
        let img = ImageBuffer::from_fn(32, 24, |x, y| {
            Rgba([(x * 8) as u8, (y * 10) as u8, 100, 255])
        });
        let out = bilateral_denoise(&img, 25);
        assert_eq!(out.dimensions(), (32, 24));
    }

    #[test]
    fn saliency_mask_full_size() {
        let img = ImageBuffer::from_fn(100, 80, |x, _| {
            if x > 40 && x < 60 {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([10, 10, 10, 255])
            }
        });
        let m = saliency_mask(&img);
        assert_eq!(m.len(), 100 * 80);
        assert!(m.iter().all(|v| (0.0..=1.0).contains(v)));
    }
}
