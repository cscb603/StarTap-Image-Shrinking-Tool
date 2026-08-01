# xTap Compress

> Multi-platform, scene-aware image compression engine in pure Rust by 星TAP实验室 (StarTAP Labs) — chat-friendly, social-feed-ready, perception-optimized. Battle-tested in production image tooling since v1.

[![crates.io](https://img.shields.io/crates/v/xtap-compress)](https://crates.io/crates/xtap-compress)
[![docs.rs](https://img.shields.io/docsrs/xtap-compress)](https://docs.rs/xtap-compress)
[![license](https://img.shields.io/crates/l/xtap-compress)](https://crates.io/crates/xtap-compress)
[![downloads](https://img.shields.io/crates/d/xtap-compress)](https://crates.io/crates/xtap-compress)

[English version](#english-version) · [中文版](#中文版--chinese)

---

## English version

A scene-aware image compression engine extracted from StarTAP Labs' production tooling (in continuous use since v1, currently v4.4.0). Unlike single-format optimizers (e.g. oxipng for PNG, mozjpeg for JPEG), **xtap-compress is a multi-format, multi-platform engine** that understands *where* the image will be used.

### Why scene-aware?

Different platforms re-compress images differently:

| Platform / Scene | English name | What it does |
|------------------|--------------|--------------|
| WeChat / WhatsApp / Telegram | Chat sharing | Defeats double-compression: target size that survives chat-app re-encoding, sharpened to compensate |
| 小红书 / Instagram / Pinterest | Social feed | Feed-optimized sizing + quality for social platforms |
| HD archive | HD archive | Maximum quality for archival / printing |
| Custom | Custom | Full manual control (max dim, quality, target KB) |
| General | General | Balanced defaults for any pipeline |

### Features

- **Multi-format**: JPEG (mozjpeg encoder, pure Rust), PNG, WebP, TIFF; RAW (DNG/CR2/CR3/NEF/ARW/ORF/RAF/RW2/PEF/SRW/3FR) on macOS
- **Scene presets**: chat / social / HD / custom / general — one call, correct settings
- **Perception-aware**: SSIM/PSNR quality metrics, face-priority saliency masking, contrast-adaptive sharpening (CAS) to compensate downscale softness
- **EXIF-friendly**: preserves metadata where supported
- **Zero heavy deps**: pure Rust, no system libraries, compiles on macOS / Windows / Linux

### Quick start

```toml
[dependencies]
xtap-compress = "0.1"
```

```rust
use xtap_compress::{
    app_config_to_process_config, AppConfig, OutputFormat, ProcessConfig, Processor,
};

// Scene presets via AppConfig: "social" (chat) / "archive" (HD) / "custom"
let app = AppConfig {
    usage_mode: "social".to_string(),  // chat-app friendly (anti double-compression)
    custom_max_dim: 1280,
    custom_quality: 90,
    custom_target_kb: 300,
    ..Default::default()
};

let config: ProcessConfig = app_config_to_process_config(&app, Some("compressed/".into()));
let processor = Processor::new(config);
let output_path = processor.process_image("photo.jpg")?;
```

Or build a `ProcessConfig` directly for full control:

```rust
use xtap_compress::{OutputFormat, ProcessConfig, Processor, ProcessMode};

let config = ProcessConfig {
    mode: ProcessMode::WeChat,
    max_dim: 1280,
    quality: 90,
    target_kb: 300,
    output_dir: Some("compressed/".into()),
    overwrite: false,
    keep_original_name: false,
    output_format: OutputFormat::Jpeg,
    color_space: xtap_compress::ColorSpace::KeepOriginal,
    enable_sharpening: false,
    sharpening_radius: 1.0,
    sharpening_amount: 0.8,
    perceptual: None, // Some(PerceptualOptions) enables SSIM/PSNR metrics + CAS sharpening
    subsampling: "420".to_string(),
    preserve_structure: false,
    structure_base: None,
    output_suffix: None,
    cas_strength: 0.0,
};

let processor = Processor::new(config);
let (output, metrics) = processor.process_image_with_metrics("photo.jpg")?;
```

### License

MIT © 2026 星TAP实验室 (StarTAP Labs)

---

## 中文版 / Chinese

多平台场景化图片压缩引擎——知道图要发到哪里，就按那个平台的规则压。纯 Rust，零系统依赖。

### 这库是干嘛的？（人话版）

**有什么用？**
图片压缩不是"压小就行"——发微信、发小红书、存高清，每个平台的规则都不一样，乱压会导致"二次压缩变糊"或者"压完还很大"。这个库**按场景自动选最优参数**，一次调用搞定。

**给我什么好处？**
- **防二压**：聊天场景（微信/WhatsApp/Telegram）压到"能扛住平台再压缩"的体积，还自动锐化补偿
- **社交平台适配**：小红书/Instagram 等平台的最优尺寸和质量
- **感知画质评估**：压完自动打分（SSIM/PSNR），量化到底好不好
- **人脸优先**：压缩时人脸区域重点保护，朋友照片不糊脸

**为什么用它？**
市面上 oxipng/mozjpeg 都是"单格式优化器"，只懂一种格式；这个是**场景化引擎**——懂格式、懂平台、懂画质。而且从 v1 到 v4.4 在生产环境用了几年，不是玩具。

**跟我有什么关系？**
写 Rust 图像工具、做图片上传/分享/存储的任何人——加一行依赖，聊天/社交/存档三种场景的压缩全搞定。

### 快速开始

```toml
[dependencies]
xtap-compress = "0.1"
```

```rust
use xtap_compress::{app_config_to_process_config, AppConfig, Processor};

// 场景预设："social"（聊天防二压）/ "archive"（高清存档）/ "custom"（自定义）
let app = AppConfig {
    usage_mode: "social".to_string(),
    custom_max_dim: 1280,
    custom_quality: 90,
    custom_target_kb: 300,
    ..Default::default()
};

let processor = Processor::new(app_config_to_process_config(&app, Some("compressed/".into())));
processor.process_image("photo.jpg")?;
```

### License

MIT © 2026 星TAP实验室
