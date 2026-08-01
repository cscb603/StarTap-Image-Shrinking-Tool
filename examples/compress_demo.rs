//! xtap-compress 独立用法演示
//!
//! 运行：cargo run --example compress_demo
//!
//! 展示库的「纯算法」发布形态：不依赖 GUI/CLI，只用 Processor 核心 API
//! 即可完成场景化图片压缩（聊天/社交/高清存档 + 感知质量评估 + CAS 锐化）。
//!
//! 本 example 程序化生成一张渐变测试图，避免依赖外部图片文件，
//! 输出到系统临时目录，并打印压缩前后体积与输出路径。

use std::path::PathBuf;
use std::time::Instant;

use xtap_compress::{app_config_to_process_config, AppConfig, ProcessConfig, Processor};

fn main() -> anyhow::Result<()> {
    // 1) 生成一张 1600x1200 渐变测试图（RGB，无外部依赖）
    let test_img_path = std::env::temp_dir().join("xtap_compress_demo_source.png");
    let (w, h) = (1600u32, 1200u32);
    let mut img = image::RgbaImage::new(w, h);
    for (x, y, px) in img.enumerate_pixels_mut() {
        let r = (x as f32 / w as f32 * 255.0) as u8;
        let g = (y as f32 / h as f32 * 255.0) as u8;
        let b = ((x + y) as f32 / (w + h) as f32 * 255.0) as u8;
        *px = image::Rgba([r, g, b, 255]);
    }
    img.save(&test_img_path)?;
    println!(
        "[1/3] 测试图已生成: {} ({}x{})",
        test_img_path.display(),
        w,
        h
    );

    // 2) 用场景化配置构造 Processor（社交分享：聊天场景防二压）
    let app_config = AppConfig {
        usage_mode: "social".to_string(),
        quality_mode: "perceptual".to_string(),
        platform: "wechat".to_string(),
        custom_max_dim: 1280,
        custom_quality: 90,
        custom_target_kb: 300,
        ..Default::default()
    };

    let output_dir = std::env::temp_dir().join("xtap_compress_demo_out");
    let process_config: ProcessConfig = app_config_to_process_config(&app_config, Some(output_dir));
    let processor = Processor::new(process_config);

    // 3) 压缩并验证出图
    let t = Instant::now();
    let out_path: PathBuf = processor.process_image(&test_img_path)?;
    let elapsed = t.elapsed();

    let out_size = std::fs::metadata(&out_path)?.len();
    let src_size = std::fs::metadata(&test_img_path)?.len();
    println!("[2/3] 压缩完成: {}", out_path.display());
    println!(
        "      耗时 {}ms | 原图 {} KB → 输出 {} KB (节省 {:.1}%)",
        elapsed.as_millis(),
        src_size / 1024,
        out_size / 1024,
        (1.0 - out_size as f64 / src_size as f64) * 100.0
    );

    // 4) 验证输出存在且可解码
    let decoded = image::open(&out_path)?;
    println!(
        "[3/3] 输出可解码 ✅ ({}x{})",
        decoded.width(),
        decoded.height()
    );
    println!(
        "\nxtap-compress v{} 示例运行成功",
        env!("CARGO_PKG_VERSION")
    );
    Ok(())
}
