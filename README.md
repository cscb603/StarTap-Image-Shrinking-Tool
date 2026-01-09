# StarTap Image Shrinking Tool（WeChat）

中文介绍

StarTap Image Shrinking Tool 是专为微信、朋友圈及网络发图打造的宝藏级缩图工具！精准攻克图片在微信发送、朋友圈发布时被二次压缩的难题，让图片 “体积小” 与 “清晰度高” 兼得，真正实现 “小而美”。
支持 JPEG、PNG 等多种格式，拖拽或点击即可轻松添加图片，自动生成适配网络传播的缩图，搭配进度条实时反馈处理进度，结果清晰明了。更提供苹果版与 Windows 版文件，无论使用何种设备，都能让你在网络发图时告别二次压缩，轻松分享清晰美照！

英文介绍

StarTap Image Shrinking Tool—a treasure - level image - shrinking utility tailored for sharing images on WeChat, Moments, and other online platforms! It effectively solves the issue of images being re - compressed when sent via WeChat or posted on Moments, ensuring images stay "small in size" while retaining "high definition"—truly achieving "small and beautiful". Supporting formats like JPEG and PNG, you can add images effortlessly via drag - and - drop or clicks. The tool automatically generates resized images for online sharing, with a progress bar for real - time feedback. We also offer Apple and Windows versions. No matter which device you use, say goodbye to re - compression worries and share clear, beautiful photos with ease!

程序具体功能说明

1. 精准尺寸适配：将图片最大尺寸缩至 2048 像素以内，适配微信等平台展示规则，从尺寸层面避免二次压缩。
2. 智能体积优化：全力将图片文件大小控制在 900KB 以内，通过渐进式调整图片质量，在保障清晰度的同时，让体积符合网络传播标准，杜绝因过大被平台压缩。
3. 锐度强化处理：缩放过程中启用锐度增强（锐度系数 1.2），确保图片缩小后依然清晰锐利，发至微信、朋友圈等场景，细节依旧精致。
4. 批量高效处理：支持一次处理多个图片文件，同步显示进度，快速完成多图优化，满足批量发图需求。
5. 极简操作体验：直观图形界面，拖放图片或点击按钮即可添加文件，零门槛操作，轻松上手。
   
需要的库

os：Python 标准库，用于文件和目录操作。
sys：Python 标准库，提供对 Python 解释器的访问与操作。
wx：wxPython 库，用于创建图形用户界面（GUI）。
PIL（Pillow）：用于图像打开、处理与保存，其中 Image 模块处理基本图像操作，ImageEnhance 模块实现图像增强。
threading：Python 标准库（代码中导入未实际使用，可用于潜在多线程优化）。

运行源代码前，需安装 wxPython 和 Pillow 库，命令如下：
bash
pip install wxPython pillow

如果是下载编译好的程序，那么
【Windows 系统使用步骤】

1 解压工具：找到压缩包，把里面的 “星TAP 缩图工具 1.0.exe” 解压到电脑（放桌面这种好找的地方就行）。
2 打开工具：双击 “星TAP 缩图工具 1.0.exe” 程序。
3 拖图压缩：把要压缩的图片直接拖进程序窗口，结束后去原图片的文件夹，找文件名带 “_xiao” 的图，发圈传图就用它！


【Mac 系统使用步骤】

安装工具（两种方法选其一）：

1 方法一：打开压缩包星 Tap 缩图工具 1.0_Mac.app.zip，直接把 “星Tap 缩图工具 1.0_Mac.app” 拖到桌面，以后双击桌面图标就能用。
2 方法二：打开压缩包，把 “星Tap 缩图工具 1.0_Mac.app” 拖进左下角 “访达” 里的 “应用程序” 文件夹。之后在启动台找到它，双击打开。
3 压缩图片：打开工具后，把图片拖进窗口，处理完去原图片文件夹，找文件名带 “_xiao” 的图，发朋友圈、微信用它超方便！

反馈方式
使用中有问题或建议，欢迎反馈到邮箱：cscb603@qq.com，祝用得顺手～

【程序说明】：
缩图大小为了适配朋友圈和网路分享，又小又清楚。

因为在微信群发图和票圈，长边大于 2048像素，大小大于900kb，微信官方将启动二次压缩，变模糊

2048与900kb是阀值，如果压到900kb画质会下降，则智能缩小图片尺寸。

如果缩小图小于1080px依然不行，则保留2048像素最高质量，不再限制900kb大小。
（以上皆是智能判断。）

最近（2025/10/23)更新到了 v2.6版。
星TAP高清缩图优化版 v2.6
——手机/微信发图神器！
咱这工具专治各种图片烦恼：

✅ 大图片发不出？自动压到微信友好的900KB左右，画质还几乎无损
✅ 缩放后模糊？用了LANCZOS黑科技，放大缩小都跟原图画质一样顶
✅ 照片有噪点？智能降噪算法，人像磨皮不糊脸，风景天空更干净
✅ 处理太慢？多线程并行狂飙，一次拖10张图也秒处理

两种模式随便挑：
  👉 微信优化模式：发圈/网络专用，体积小传输快，对方看着还清晰
  👉 无损缩图模式：想保留高清细节？这个模式直接拉满画质，文件稍大

【傻瓜式操作】：

解压到桌面打开，
拖张图进来就行，剩下的交给程序，
处理后的图在原图片的文件夹。
连小白都能玩明白～

# StarTap Image Shrinking Tool (Rust v3.0 High-Performance Edition)

## 🚀 2026 全新 Rust 引擎升级版

**StarTap Image Shrinking Tool** 现已全面升级至 Rust 引擎！专为微信、朋友圈及网络发图打造的“宝藏级”缩图工具。在保留原有智能逻辑的基础上，利用 Rust 原生性能实现了秒级的处理速度，让您的图片在网络分享时真正实现“小而美”。

---

### 🇨🇳 中文介绍

精准攻克图片在微信发送、朋友圈发布时被二次压缩的难题。Rust 版本通过 **SIMD 指令集加速**和**多线程并行处理**，让即便上千张的大图处理也能稳如泰山。它智能判断 2048px 与 900KB 的阈值，通过渐进式压缩算法，确保“体积小”与“清晰度高”完美兼得，彻底告别微信模糊图。

### 🇺🇸 English Introduction

**StarTap Image Shrinking Tool** has been fully re-engineered with Rust! Specifically tailored for WeChat and social media sharing. The Rust version leverages **SIMD acceleration** and **multi-threaded parallel processing**, delivering lightning-fast speeds even for thousands of high-res images. It intelligently balances the 2048px/900KB threshold using progressive compression, ensuring your photos stay "small in size" but "high in definition"—truly achieving "small and beautiful" without the worries of platform re-compression.

---

### 🛠️ 核心功能亮点 (Rust v3.0)

1.  **极速并行处理**：基于 Rayon 并行框架，自动榨干 CPU 每一核性能，多图处理速度提升 5-10 倍。
2.  **智能尺寸适配**：严格遵循 2048px 黄金准则，规避平台二次压缩。支持自定义 3:4 智能裁剪。
3.  **双重优化模式**：
    *   **微信优化模式**：精准控制在 900KB 以内，社交分享首选。
    *   **无损高清模式**：保留最高画质细节，适合专业摄影展示。
4.  **新增进阶控制**：
    *   **覆盖原图**：支持直接替换原始文件（带安全确认弹窗）。
    *   **保持原名**：导出到其他文件夹时可选择不增加后缀，保持干净的文件名。
5.  **工业级稳定性**：自动跳过损坏图片，支持 1000+ 数量级的批量任务不卡死、不崩溃。
6.  **锐度强化 (LANCZOS)**：采用最高等级的重采样算法，缩小后的图片比原图更锐利。

---

### 💻 技术栈 (Tech Stack)

*   **Language**: Rust (2024/2026 Edition)
*   **GUI**: egui / eframe (原生硬件加速界面)
*   **Processing**: Image crate + fast_image_resize (SIMD 加速)
*   **Parallelism**: Rayon (多线程)
*   **Compiler Optimization**: sccache + LTO + Profile-Guided Optimization

--- 

### 🚀 使用指南

#### 【Windows 系统】
1.  **解压即用**：把exe或app 打开即用`在当前项目code最新文件里有，。
2.  **拖拽处理**：将图片或整个文件夹直接拖入窗口。
3.  **灵活配置**：在界面左侧勾选“覆盖原图”或“保持原名”，点击开始即可。
4.  **查看结果**：默认在原图旁生成 `_opt` 后缀文件，或直接覆盖。处理完会自动弹出结果目录。

#### 【开发者/编译步骤】
如果您想从源代码构建以获得极致性能：
```powershell
# 确保已安装 Rust 环境
cargo build --release
```
*注：本项目已配置 `.cargo/config.toml` 自动启用 `sccache` 加速编译。*

---

### 📅 更新日志

*   **v3.0 (2026/01/09)**：
    *   由 Python 迁移至 Rust，处理性能质的飞跃。
    *   新增“覆盖原图”及“保持原名”功能选项。
    *   引入智能下采样分析，识别大图内容速度提升 50 倍。
    *   修复了旧版处理损坏图片时可能卡死的问题。
*   **v2.6 (2025/10/23)**：
    *   优化了微信 900KB 阈值的智能判断逻辑。

---

### 📮 反馈与支持

在使用中有任何建议或遇到 Bug，欢迎反馈：
📧 邮箱：**cscb603@qq.com**

> **程序说明**：缩图大小为了适配朋友圈和网络分享，又小又清楚。2048px 与 900KB 是微信压缩的生死线，交给 StarTap，您只管分享美好！
