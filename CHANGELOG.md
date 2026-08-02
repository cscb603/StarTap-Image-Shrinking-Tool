# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.1] - 2026-08-02

### Fixed

- 修复 4 处 rustdoc broken intra-doc link（`[0,1]` / `[i]` 数学下标被误解析为链接）

### Added

- GitHub Actions CI（`.github/workflows/ci.yml`）：fmt + build + test + clippy `-D warnings` 官方门禁

[0.1.1]: https://crates.io/crates/xtap-compress

## [0.1.0] - 2026-08-01

### Added
- **xtap-compress** 首个发布版本：从星TAP实验室生产工具（高清缩图 v4.4.0）抽取的通用图片压缩引擎库
- 纯算法发布形态：`default = []`，用户 `cargo add xtap-compress` 零 GUI/CLI 重依赖
- 核心 API：
  - `Processor` / `ProcessConfig` / `app_config_to_process_config` — 场景化压缩入口
  - `ProcessMode`（WeChat/HD/Custom）/ `OutputFormat`（Jpeg/WebP/KeepOriginal）/ `ColorSpace`
- 场景预设：`AppConfig.usage_mode` = social（聊天防二压）/ archive（高清存档）/ custom（自定义）
- 感知压缩（`perceptual` 模块）：SSIM/PSNR 指标、人脸优先显著性掩码、CAS 自适应锐化
- 多格式：JPEG（mozjpeg 纯 Rust 编码器）/ PNG / WebP / TIFF；RAW（macOS）
- EXIF 保留：APP0-APP14 全段元数据零损失
- 示例：`examples/compress_demo.rs` 独立压缩出图演示
- 单测：13 个（含 lib 核心 5 个新增 + cas/perceptual 既有 8 个）
