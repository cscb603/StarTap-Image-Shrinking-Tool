//! v4.4.0：CAS 对比度自适应锐化（Contrast Adaptive Sharpening）
//!
//! 思路源自 AMD FidelityFX CAS 简化十字变体（MIT 协议公开算法），纯 Rust 手写，零新依赖。
//! 专为「降采样后补偿锐度」设计，与 USM 的本质区别：
//! - 输出 = 十字邻域加权平均（负权重中心增强），**构造上不产生过冲**——
//!   结果永远落在邻域 min/max 附近，没有 USM 的白边/黑边光晕，观感自然。
//! - 逐像素自适应：局部对比度越高（已经很锐的边），锐化权重越小；
//!   平坦区（天空/皮肤）邻域和 = 4×中心值，输出恒等于原值——天然内容感知。
//! - 近黑/近白区域自动收敛权重，防裁剪断层。
//!
//! strength ∈ `[0,1]`：映射 AMD 官方 knob，峰值负权重在 -1/8（弱）~ -1/5（强）
//! 之间插值。0.3~0.4 为「自然补偿」区间（本工具画质优先档默认 0.35）。

use image::DynamicImage;
use rayon::prelude::*;

/// 对图像做 CAS 锐化，返回新图。strength<=0 或图过小时原样返回。
/// 在 sRGB 空间逐通道直算（与本工程 USM 同约定），alpha 通道透传。
pub fn cas_sharpen(img: &DynamicImage, strength: f32) -> DynamicImage {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if strength <= 0.0 || w < 3 || h < 3 {
        return img.clone();
    }
    let sharp = strength.clamp(0.0, 1.0);
    // AMD CAS 官方映射：sharp=0 → peak=-1/8（温和），sharp=1 → peak=-1/5（最强）
    let peak = -1.0 / (8.0 - 3.0 * sharp);

    let src = rgba.as_raw();
    let stride = (w as usize) * 4;
    let mut out = vec![0u8; src.len()];

    out.par_chunks_mut(stride).enumerate().for_each(|(yi, row)| {
        let y = yi as u32;
        let ym = y.saturating_sub(1);
        let yp = (y + 1).min(h - 1);
        for x in 0..w {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(w - 1);
            let base = (x as usize) * 4;
            let px = |xx: u32, yy: u32, c: usize| -> f32 {
                src[(yy as usize) * stride + (xx as usize) * 4 + c] as f32 * (1.0 / 255.0)
            };
            for c in 0..3 {
                let m = px(x, y, c);
                let n = px(x, ym, c);
                let s = px(x, yp, c);
                let e = px(xp, y, c);
                let wv = px(xm, y, c);
                let mn = m.min(n).min(s).min(e).min(wv);
                let mx = m.max(n).max(s).max(e).max(wv);
                // 自适应量：局部对比度高 → d 小 → 少锐化（防已锐边过冲）；
                // 近白（2-mx 小）同样收敛，防高光裁剪。
                let d = if mx > 1e-6 {
                    (mn.min(2.0 - mx) / mx).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let wgt = d.sqrt() * peak;
                let v = ((n + s + e + wv) * wgt + m) / (4.0 * wgt + 1.0);
                row[base + c] = (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            }
            // alpha 透传
            row[base + 3] = src[(y as usize) * stride + base + 3];
        }
    });

    DynamicImage::ImageRgba8(
        image::ImageBuffer::from_raw(w, h, out).expect("CAS 输出缓冲尺寸恒等于输入"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    /// 平坦图（纯色）经 CAS 后必须逐字节不变（内容自适应：平坦区零锐化）
    #[test]
    fn flat_image_unchanged() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            16,
            16,
            Rgba([120, 130, 140, 255]),
        ));
        let out = cas_sharpen(&img, 0.35);
        assert_eq!(img.to_rgba8().as_raw(), out.to_rgba8().as_raw());
    }

    /// 垂直边缘经 CAS 后局部对比度应提升（锐化确实生效），且无越界 panic
    #[test]
    fn edge_contrast_increases() {
        let mut buf = RgbaImage::from_pixel(16, 16, Rgba([80, 80, 80, 255]));
        for y in 0..16 {
            for x in 8..16 {
                buf.put_pixel(x, y, Rgba([170, 170, 170, 255]));
            }
        }
        let img = DynamicImage::ImageRgba8(buf);
        let out = cas_sharpen(&img, 0.35).to_rgba8();
        let src = img.to_rgba8();
        // 边界两侧像素：暗侧应更暗或持平、亮侧应更亮或持平（对比度非降）
        let dark_src = src.get_pixel(7, 8)[0] as i32;
        let dark_out = out.get_pixel(7, 8)[0] as i32;
        let lite_src = src.get_pixel(8, 8)[0] as i32;
        let lite_out = out.get_pixel(8, 8)[0] as i32;
        assert!(dark_out <= dark_src, "暗侧不应变亮: {dark_src} -> {dark_out}");
        assert!(lite_out >= lite_src, "亮侧不应变暗: {lite_src} -> {lite_out}");
        assert!(
            (lite_out - dark_out) > (lite_src - dark_src),
            "边缘对比度应提升"
        );
        // 无过冲铁律：输出不得超出邻域值域外太多（CAS 构造保证）
        assert!(dark_out >= 60 && lite_out <= 190, "不应产生 USM 式过冲光晕");
    }

    /// strength=0 原样返回
    #[test]
    fn zero_strength_noop() {
        let img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            8,
            8,
            Rgba([10, 200, 90, 128]),
        ));
        let out = cas_sharpen(&img, 0.0);
        assert_eq!(img.to_rgba8().as_raw(), out.to_rgba8().as_raw());
    }
}
