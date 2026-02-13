# 🖼️ 星TAP | 高清缩图 RUST 优化版 (StarTap Image Shrinking Tool)

[![GitHub release](https://img.shields.io/github/v/release/cscb603/StarTap-Image-Shrinking-Tool?include_prereleases)](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS-blue)](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases)

**专为微信、朋友圈及网络发图打造的宝藏级缩图工具！**

精准攻克图片在微信发送、朋友圈发布时被二次压缩的难题，让图片 “体积小” 与 “清晰度高” 兼得，真正实现 “小而美”。

## 📸 界面预览 (GUI Preview)

![macOS 3.2.0 界面](rust版高清缩图界面mac版界面.jpg)
![Windows 界面演示](rust版高清缩图界面1.jpg)

---

## ✨ 2026 年 2 月重大升级 (2026 February Major Upgrade - v3.2)

- 🦀 **工业级 Rust 内核 v3.2**：基于 2026 最新 Rust 标准构建，开启 **LTO (Link Time Optimization)** 全局优化，处理吞吐量提升约 40%。
- 🚀 **标准 macOS App 封装**：现已提供标准的 `.app` 应用程序包，支持 **双击直接运行**，告别命令行操作。
- 🌈 **智能透明度处理**：完美解决 PNG/WebP 透明背景转 JPEG 时的混合逻辑，边缘更加顺滑，无黑边困扰。
- 💾 **无损元数据保留**：重构了 JPEG 编码流，100% 保留拍摄器材、GPS 等 EXIF 原始信息。
- ⚡ **硬件加速渲染**：界面采用 `wgpu` 硬件加速，UI 响应零延迟，操作丝滑顺畅。
- 📉 **体积极致优化**：剔除冗余调试符号，App 包体积更精简，每一 KB 空间都为性能而生。

---

## ✨ 2026 年 1 月内核重构 (2026 January Kernel Reconstruction)

---

## 🛠️ 核心黑科技 (Core Features)

- ✅ **微信友好优化**：自动将图片压至 900KB 左右（微信朋友圈无损上传临界点），画质几乎无损。
- ✅ **LANCZOS 高级采样**：采用黑科技算法，确保缩放后的图片与原图画质一样顶，告别模糊。
- ✅ **智能降噪**：内置智能算法，人像磨皮不糊脸，风景天空更干净。
- ✅ **两种模式随心选**：
  - 👉 **微信优化模式**：发圈/网络专用，体积小、传输快、清晰度高。
  - 👉 **无损缩图模式**：保留更多细节，适合对画质有极致要求的场景。

---

## 🚀 快速上手 (Quick Start)

1. **下载**：前往 **[Releases 页面](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases)** 下载对应系统的压缩包。
2. **运行**：解压到桌面，双击打开程序。
3. **操作**：直接将图片拖入程序界面。
4. **完成**：处理后的图片会自动保存在原图片所在的文件夹中。

---

## 🤝 联系与支持 (Contact)

- **作者**：星TAP
- **GitHub**: [cscb603/StarTap-Image-Shrinking-Tool](https://github.com/cscb603/StarTap-Image-Shrinking-Tool)
- 如果觉得好用，请点击右上角的 **Star** ⭐！

---

**English Version Summary**
StarTap Image Shrinking Tool is a professional utility designed for social media and web optimization. Powered by a brand-new **Rust kernel**, it offers high-speed batch processing with support for RAW formats (CR2, CR3, DNG). It features LANCZOS scaling and intelligent noise reduction to ensure your images stay sharp even after significant compression, making them perfect for WeChat and other platforms.
