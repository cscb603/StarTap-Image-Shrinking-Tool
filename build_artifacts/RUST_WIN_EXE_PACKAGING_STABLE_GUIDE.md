# Rust + egui + Windows EXE 极致打包与 UI 设计标准指南 (SOP)

> **记忆自动化提示**：此文档为“星TAP 实验室”长期记忆库核心文件。未来所有基于 Rust 的 GUI 项目必须强制遵循此 SOP 路径，以确保零弯路、零乱码、高颜值。

## 🛠 核心链路：最稳打包流程 (The "Golden Path")

### 1. 环境准备
- **图标源文件**：准备一张 `512x512` 或更大的透明 PNG (建议命名为 `source_icon.png`)。
- **资源脚本**：使用 `resources.rc` 统一管理图标、清单和版本信息。

### 2. 图标生成 (零白边方案)
**不要**直接使用在线转换工具。使用项目内的 `generate_icon_v2.py`：
- **原理**：利用 PIL 的 `LANCZOS` 采样 + `rounded_rectangle` 遮罩。
- **运行**：`python generate_icon_v2.py`
- **产物**：`icon.ico` (包含 16/32/48/64/128/256 全尺寸，自带抗锯齿圆角)。

### 3. Windows 属性与语言 (避坑指南)
在 `resources.rc` 中必须严格执行：
- **UTF-8 声明**：`#pragma code_page(65001)`。
- **语言代码**：
    - `Translation` 设为 `0x0804, 1200` (简体中文, Unicode)。
    - `StringFileInfo` 的 Block 必须对应 `080404b0`。
- **清单引用**：必须包含 `1 24 "app.manifest"` 以支持 DPI 缩放，解决 4K 屏模糊问题。

### 4. 编译与打包
```powershell
# 1. 生成图标
python generate_icon_v2.py

# 2. 生产环境编译 (极致体积优化)
cargo build --release

# 3. 最终重命名 (保持原始文件名属性一致)
Move-Item -Path "target\release\rust_image_compressor.exe" -Destination "target\release\星TAP 高速缩图rust版.exe" -Force
```

---

## 🎨 UI/UX 设计规范 (UI UX Pro Max 实践)

### 1. 字体与排版
- **字体加载**：强制加载 `Microsoft YaHei Light`。
    - *经验*：标准微软雅黑在小字号下锯齿感重，Light 版本在 SaaS 风格中更显高级。
- **呼吸感**：`inner_margin` 统一使用 `symmetric(40.0, 25.0)`。
- **留白**：内容区与边框必须有明确的灰色分割线 (`Slate 100`) 和背景色区分 (`Slate 50`)。

### 2. 交互反馈 (小白友好)
- **状态感知**：处理时必须显示“当前文件名”和“百分比”。
- **自动逻辑**：
    - 拖拽支持：文件、文件夹混合拖拽。
    - 递归扫描：自动处理子目录。
    - 智能弹出：完成后自动 `opener::open` 结果目录。

---

## 🧹 冗余清理记录 (长期记忆)
- **已弃用工具**：`generate_icon.py` (旧版，缩放质量差，有白边)。
- **已弃用脚本**：`icon.rc` (与 `resources.rc` 冲突)。
- **不建议路径**：在 Rust 内部直接硬编码图标字节 (会导致资源管理器无法抓取图标)，必须通过 `resources.rc` 嵌入。

## 🚀 未来扩展
- **Mac 适配**：若需支持 Mac，需额外准备 `.icns` 文件并修改 `Cargo.toml` 的 `bundle` 配置，但 `main.rs` 中的 `PathBuf` 逻辑已通用。

---
**星TAP 实验室 · 2026**
