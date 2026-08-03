# 星TAP 图片高速压缩 · StarTap Image Shrinking Tool

> **微信图片压缩 · 防二次压缩 · Rust 图像优化 · WeChat / WhatsApp / Xiaohongshu Anti-Recompression**
> 一个**本地离线运行**的极速图片压缩工具，专为"干掉"微信、朋友圈、小红书、WhatsApp 的暴力二次压缩而生。

[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue)](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)
[![Latest Release](https://img.shields.io/github/v/release/cscb603/StarTap-Image-Shrinking-Tool?label=version)](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases)
[![Rust](https://img.shields.io/badge/engine-Rust%20%2B%20mozjpeg-orange)](https://crates.io/crates/xtap-compress)

---

## English version

**StarTap is a Rust-based local image shrinking tool designed specifically to defeat the aggressive secondary compression of platforms like WeChat, WhatsApp, and Xiaohongshu. It uses mozjpeg and CAS to preserve DSLR-level image quality under strict size limits.**

- 🏠 **100% local & offline.** Your photos never leave your machine. No upload, no privacy leak.
- 🛡️ **Anti-recompression engine.** Pre-compresses images to *exactly* the size each platform accepts — so WeChat / Xiaohongshu re-encode them again without destroying quality.
- 🎨 **Quality-first by default.** Opens in `Max` mode: Q96 + **4:4:4 full chroma** + **CAS (Contrast-Adaptive Sharpening)**. You do nothing, you get the best.
- 🤖 **Agent-First CLI.** Both a human GUI and a standard AI interface. LLM agents call it via a clean JSON envelope — perfect for AI workflows, RAG pipelines, and automation.
- ⚡ **Pure Rust, single binary.** Zero heavy dependencies. ~4.5 MB on Windows, native on macOS.

### Tags / Keywords (for search & AI retrieval)

`WeChat Image Compression` · `Anti-Recompression` · `Rust Image Optimizer` ·
`mozjpeg` · `Lossless / Lossy JPEG` · `4:4:4 chroma` · `CAS sharpening` ·
`Xiaohongshu compression` · `WhatsApp image compression` · `Instagram compression` ·
`local image compressor` · `offline photo shrinker` · `AI-callable image API` ·
`command-line image compression` · `Agent-First JSON CLI` · `cross-platform`

### Why does this exist?

You take a great photo. You send it on WeChat → it becomes blurry. You post it on Xiaohongshu → the colors wash out. **That's not your camera's fault — it's the platform re-compressing your image.**

StarTap flips the game: it pre-processes the photo *according to each platform's exact rules* **before** you upload. The file is small enough to dodge re-compression, yet pushed to the maximum quality the platform allows — full color, sharp detail, no "double-squash" blur.

### Quick start (CLI)

```bash
# Human quick use — shrink a folder, default = quality-first + WeChat-safe
./图片高速压缩 "C:/Photos"

# AI / script use — the most stable way (JSON envelope, no shell escaping)
echo '{"files":["C:/Photos/a.jpg","C:/Photos/b.jpg"],"platform":"wechat","quality_mode":"max"}' \
  | ./图片高速压缩 --json

# Or pass JSON directly
./图片高速压缩 --json-in '{"files":["photo.jpg"],"platform":"xiaohongshu"}'

# Streaming JSONL: one result line per file, plus a summary envelope at the end
./图片高速压缩 --json-in '{"files":["p1.jpg","p2.jpg"],"jsonl":true}'
```

Exit codes: `0` = all ok · `1` = some failed · `2` = bad arguments.
Idempotent by default (existing outputs are skipped); use `--force` to re-run.

### Platform presets

| Preset | Platform | What it does |
|--------|----------|--------------|
| `wechat` / `wechat-new` | WeChat / Moments | Anti-recompression sizing for chat & Moments |
| `xiaohongshu` | RED / Xiaohongshu | 1660px long-edge, feed-optimized quality |
| `instagram` | Instagram | Square/portrait feed sizing |
| `general` | Any pipeline | Balanced defaults, no forced sRGB |
| custom | You decide | Full manual: max-dim, quality, target KB |

### For developers & AI agents

The compression core is published as a Rust crate **`xtap-compress`** ([crates.io](https://crates.io/crates/xtap-compress) · [docs.rs](https://docs.rs/xtap-compress)). The CLI wraps it with an **Agent-First JSON envelope**:

- `--json` / `--json-in` — pass a JSON object (no shell-escaping headaches)
- `--jsonl` — streaming, one JSON line per file + a summary envelope
- `--quality-mode max|perceptual|normal` — `max` = anti-recompression quality-first
- `--cas-strength 0..1` — control sharpening intensity (default 0.35 in quality-first)
- `--output-format jpeg|webp|original` — WebP for smaller size / transparency

```rust
// Library usage (xtap-compress)
use xtap_compress::{AppConfig, Processor};
let app = AppConfig { usage_mode: "social".into(), ..Default::default() };
let cfg = app_config_to_process_config(&app);
Processor::new(cfg).process_paths(&["photo.jpg"]);
```

---

## 中文版 · Chinese

**星TAP 图片高速压缩** 是一个**本地离线**运行的极速图片压缩工具，用 Rust 写成，专门解决一个让人抓狂的问题：**你精心拍的照片，一发微信 / 朋友圈 / 小红书就变糊了。**

### 🏷 关键词标签（方便搜索与 AI 检索）

`微信图片压缩` · `防二次压缩` · `朋友圈画质模糊` · `Rust 图像优化` ·
`mozjpeg` · `无损/有损 JPEG` · `4:4:4 色度` · `CAS 锐化` ·
`小红书图片压缩` · `WhatsApp 图片压缩` · `Instagram 压缩` ·
`本地图片压缩工具` · `离线图片瘦身` · `AI 可调用的图片压缩` ·
`命令行图片压缩` · `Agent-First JSON 接口` · `跨平台`

### 为什么需要它？

你拍了好照片 → 微信发出去变糊 → 小红书发出来掉色。**这锅不该你的相机背，是平台在偷偷把你的图重新压了一遍。**

星TAP 的思路是反过来的：在你上传**之前**，先按**每个平台的规则**把图处理到位。文件小到不会触发平台的二次压缩，画质却顶到它能接受的天花板——色彩饱满、细节清晰，不再被"翻压"变糊。

### ✨ 核心特性

- **🛡️ 防二次压缩引擎**：针对不同平台预设（微信 / 小红书 / Instagram / 通用），一次调用就选对参数。
- **🎨 画质优先（默认开启）**：打开就是 `Max` 档——Q96 起步 + **4:4:4 全色度保留** + **CAS 自然锐化补偿**。你什么都不用做，直接拿最好。
- **🌟 小而美感知压缩**：同体积下画质更好（SSIM / PSNR 评估、人脸优先显著性遮罩、对比度自适应锐化 CAS）。
- **🤖 双模式：人用 GUI + AI 用 CLI**：人类拖拽即用；AI / Agent 通过标准 **JSON 信封**调用，完美接入 AI 工作流、RAG 管线与自动化。
- **📦 多格式**：JPEG（**mozjpeg 编码器，纯 Rust**）/ PNG / WebP；macOS 支持 RAW（DNG/CR2/CR3/NEF/ARW…）。
- **🔒 本地离线、隐私无忧**：处理全程在你电脑上，不上传任何图片。
- **⚡ 单文件、跨平台**：Windows 约 4.5 MB 单文件；macOS 原生运行。

### 🚀 三步上手

**最简单：拖拽。** 把照片拖进窗口，它按"画质优先 + 微信"默认档直接处理好，输出在 `compressed` 文件夹。

**换平台？** 上方"平台"切一下：微信 / 小红书 / Instagram / 通用。

**命令行？**

```bash
# 压缩整个文件夹（默认：画质优先 + 微信安全尺寸）
./图片高速压缩 "C:/照片"

# AI / 脚本最稳的写法（JSON 信封，彻底绕开 shell 转义）
echo '{"files":["C:/照片/a.jpg","C:/照片/b.jpg"],"platform":"wechat","quality_mode":"max"}' \
  | ./图片高速压缩 --json

# 直接传 JSON 字符串
./图片高速压缩 --json-in '{"files":["photo.jpg"],"platform":"xiaohongshu"}'

# 流式 JSONL：每处理完一个文件输出一行 JSON，末尾追加汇总信封
./图片高速压缩 --json-in '{"files":["p1.jpg","p2.jpg"],"jsonl":true}'
```

> 退出码：`0`=全部成功 · `1`=有失败 · `2`=参数错误。
> 默认**幂等**（已存在的输出自动跳过）；`--force` 强制重压。

### 📋 平台预设对照

| 预设 | 适用平台 | 作用 |
|------|----------|------|
| `wechat` / `wechat-new` | 微信 / 朋友圈 | 聊天 & 朋友圈防二压尺寸 |
| `xiaohongshu` | 小红书 | 1660px 长边、信息流优化画质 |
| `instagram` | Instagram | 方图 / 竖图信息流尺寸 |
| `general` | 任意管线 | 均衡默认，不强制转 sRGB |
| custom | 自定义 | 完全手动：最长边 / 质量 / 目标 KB |

### 🤖 给开发者与 AI 的接口

压缩内核已发布为 Rust crate **`xtap-compress`**（[crates.io](https://crates.io/crates/xtap-compress) · [docs.rs](https://docs.rs/xtap-compress)）。CLI 在它外面套了一层 **Agent-First JSON 信封**：

- `--json` / `--json-in`：传入 JSON 对象（没有 shell 转义烦恼）
- `--jsonl`：流式输出，每文件一行 JSON + 末尾汇总信封
- `--quality-mode max|perceptual|normal`：`max`=防二压画质优先
- `--cas-strength 0..1`：控制锐化强度（画质优先档默认 0.35）
- `--output-format jpeg|webp|original`：WebP 更省体积、支持透明

```rust
// 库调用示例（xtap-compress）
use xtap_compress::{AppConfig, Processor};
let app = AppConfig { usage_mode: "social".into(), ..Default::default() };
let cfg = app_config_to_process_config(&app);
Processor::new(cfg).process_paths(&["photo.jpg"]);
```

---

## 📥 下载 / Download

| 平台 | 下载地址 |
|------|----------|
| 🪟 Windows | [蓝奏云 wwbfk.lanzoub.com/iCsRs40cdgq](https://wwbfk.lanzoub.com/iCsRs40cdgq) · [GitHub Release（备用）](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases/latest) |
| 🍎 macOS | [蓝奏云 wwbfk.lanzoub.com/iRrs340cdfr](https://wwbfk.lanzoub.com/iRrs340cdfr) · [GitHub Release（备用）](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases/latest) |

- **Windows**：双击即跑，零安装。
- **macOS**：解压即用（若打不开请看 [这篇说明](https://juejin.cn/post/7668479808846643250)）。

> 全部处理在本地完成，**不上传任何图片**。

---

## 🧠 技术架构（简述）

```
        ┌─────────────┐         ┌─────────────┐
        │  GUI 模式    │         │  CLI / JSON  │
        │  (人类用户)   │         │  (AI / Agent)│
        └──────┬──────┘         └──────┬──────┘
               │                       │
               └───────────┬───────────┘
                           ▼
                ┌─────────────────────────┐
                │   Rust 压缩内核          │
                │   (crate: xtap-compress) │
                │  · mozjpeg 编码器         │
                │  · 4:4:4 全色度保留       │
                │  · CAS 自适应锐化         │
                │  · 感知压缩 (SSIM/PSNR)   │
                │  · 平台防二压阈值          │
                └────────────┬────────────┘
                             ▼
                   JPEG / WebP / PNG 输出
```

上层分叉，底层合一：GUI 和 CLI 只是两套"皮肤"，压缩引擎只有一份——所以**人看到的画质，和 AI 调出来的完全一致**。

---

## 📚 相关链接

- 仓库：[cscb603/StarTap-Image-Shrinking-Tool](https://github.com/cscb603/StarTap-Image-Shrinking-Tool)
- 压缩内核 crate：[xtap-compress on crates.io](https://crates.io/crates/xtap-compress)
- 深度技术长文：《受够了微信的暴力压缩？我用 Rust 写了一个支持 4:4:4 采样的防压缩引擎》

---

## License

[MIT](LICENSE) © 星TAP实验室 (StarTAP Labs)
