//! v4.2.0-exp 感知量化表验证集成测试
//!
//! 目的：在「除量化表外一切不变」的条件下，对比 v4.1.0 标准表 / 内置 CustomMsSsim / 自算 CSF
//! 三者的输出体积与 SSIM/PSNR（均与同一降采样原图参考帧对齐，算法与工具内部 to_gray/ssim_gray 同源）。
//! 运行：`cargo test --test verify_perceptual -- --nocapture`
//! 缺失 test_images 时自动跳过，不阻塞编译。

use image::GenericImageView;
use rust_image_compressor::perceptual::{FocusMode, PerceptualOptions, QuantMode};
use rust_image_compressor::{app_config_to_process_config, AppConfig, ProcessMode, Processor};
use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// 构造处理器：max_dim/quality 固定，quant_mode=None 即纯 v4.1.0 旧路径（perceptual 恒 None）
fn build_processor(
    quant_mode: Option<QuantMode>,
    out_dir: &Path,
    max_dim: u32,
    quality: u8,
) -> Processor {
    let cfg = AppConfig {
        mode: ProcessMode::Custom,
        custom_max_dim: max_dim,
        custom_quality: quality,
        overwrite: false, // 关键：true 会回写原图路径，危险
        ..Default::default()
    };
    let mut pc = app_config_to_process_config(&cfg, Some(out_dir.to_path_buf()));
    // app_config_to_process_config 恒置 perceptual=None，这里复刻 main.rs 的覆盖逻辑
    pc.perceptual = quant_mode.map(|qm| PerceptualOptions {
        denoise_strength: 0, // JPG 输入本就跳过降噪，置 0 显式排除降噪变量
        focus_mode: FocusMode::Auto,
        quality_ceil: 95,
        quant_mode: qm,
        budget_kb: None,
        platform: None,
    });
    Processor::new(pc)
}

/// 与原图（降采样到输出尺寸）对齐的 SSIM/PSNR，独立于工具内部参考帧，保证各模式横向可比
fn ssim_psnr_vs_original(orig: &Path, out: &Path) -> Option<(f64, f64)> {
    let orig_img = image::open(orig).ok()?;
    let out_img = image::open(out).ok()?;
    let (ow, oh) = out_img.dimensions();
    let orig_resized = image::imageops::resize(
        &orig_img.to_rgb8(),
        ow,
        oh,
        image::imageops::FilterType::Triangle,
    );
    let orig_dyn = image::DynamicImage::ImageRgb8(orig_resized);
    let (ref_gray, _, _) = rust_image_compressor::perceptual::to_gray(&orig_dyn);
    let (out_gray, gw, gh) = rust_image_compressor::perceptual::to_gray(&out_img);
    if gw != ow as usize || gh != oh as usize {
        return None;
    }
    let ssim = rust_image_compressor::perceptual::ssim_gray(&ref_gray, &out_gray, gw, gh);
    let psnr = rust_image_compressor::perceptual::psnr_gray(&ref_gray, &out_gray);
    Some((ssim, psnr))
}

#[test]
fn verify_perceptual_quant_tables() {
    let img = manifest_dir().join("test_images/GYL_3359.JPG");
    if !img.exists() {
        eprintln!("[verify] test_images/GYL_3359.JPG 缺失，跳过感知量化表验证");
        return;
    }
    let max_dim = 2048u32;
    let quality = 85u8;
    let base_out = std::env::temp_dir().join("v420_verify");
    let _ = std::fs::create_dir_all(&base_out);

    let modes: Vec<(&str, Option<QuantMode>)> = vec![
        ("v410_standard", None),
        ("standard", Some(QuantMode::Standard)),
        ("msssim", Some(QuantMode::MsSsim)),
        ("csf", Some(QuantMode::Csf)),
    ];

    println!(
        "\n===== v4.2.0-exp 感知量化表验证 (img={}, max_dim={}, quality={}) =====",
        img.file_name().unwrap().to_string_lossy(),
        max_dim,
        quality
    );
    println!(
        "{:<14} {:>12} {:>10} {:>10} {:>10}",
        "mode", "size_bytes", "ssim", "psnr_db", "lib_ssim"
    );
    for (label, qm) in modes {
        let out_dir = base_out.join(label);
        let _ = std::fs::create_dir_all(&out_dir);
        let proc = build_processor(qm, &out_dir, max_dim, quality);
        let (out_path, metrics) = match proc.process_image_with_metrics(&img) {
            Ok(r) => r,
            Err(e) => {
                println!("{:<14} 处理失败: {}", label, e);
                continue;
            }
        };
        let size = std::fs::metadata(&out_path).map(|m| m.len()).unwrap_or(0);
        let (ssim, psnr) = ssim_psnr_vs_original(&img, &out_path).unwrap_or((f64::NAN, f64::NAN));
        let lib_ssim = metrics
            .as_ref()
            .map(|m| m.ssim_vs_source)
            .unwrap_or(f64::NAN);
        println!(
            "{:<14} {:>12} {:>10.6} {:>10.2} {:>10.6}",
            label, size, ssim, psnr, lib_ssim
        );
    }
    println!("===== end =====\n");
}
