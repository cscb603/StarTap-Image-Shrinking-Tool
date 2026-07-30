# 星TAP 高清缩图 v4.3.1 — 工程成熟度补强，批量使用不再踩坑

v4.3.0 把底层编码器换成 mozjpeg、画质与色彩保真都上了台阶。v4.3.1 不碰编码器核心，而是在**真实个人批量使用**场景下补强工程成熟度：修掉 4 个会让人抓狂的坑（D1–D4），并把 AI 接口的诊断能力补齐。

## 这次修了什么

### D1 · 系统隐藏文件不再污染结果（自动化毒药）
- **现象**：macOS 在拷贝/压缩目录时会生成 `._xxx.jpg` 资源叉文件。旧版把它们误判为图片 → 压缩失败 → **退出码变成 1**，让上层自动化脚本以为整批失败、反复重试。
- **修复**：`process_or_passthrough` 在入口先识别 `._` 前缀，归类为 `skipped`（success=true、不计入 failed、退出码恒为 0）。真正失败时退出码才为 1。

### D2 · 输出保结构（可选）
- **现象**：指定 `output_dir` 时所有图拍平进同一个目录，多层级相册的目录信息全丢；后缀写死 `_wx/_hd/_da`，想自定义不行。
- **修复**：
  - `--preserve-structure`（CLI）/ `preserve_structure`（JSON）：以所有输入的最长公共祖先为基准，把源目录层级原样复刻到输出目录。
  - `--output-suffix <str>`（CLI）/ `output_suffix`（JSON）：自定义后缀，覆盖默认 `_wx/_hd/_da`；空串 = 无后缀。`--keep-original-name` 优先级更高。
  - 内核 `expected_output_path` 重写，支持保结构 + 可控后缀 + WebP 扩展名同源计算。

### D3 · 不支持格式原样透传（可选）
- **现象**：遇到 SVG 等不支持格式直接报失败，批量任务被迫中断或预处理。
- **修复**：新增 `--passthrough-unsupported`（CLI）/ `passthrough_unsupported`（JSON）。开启后，不支持格式（如 SVG）原样复制到输出目录、结果标记 `status:"passthrough"`，不压缩、不报失败。未开启时仍按 `unsupported` 失败，行为明确。

### D4 · 透明 PNG 不再丢透明
- **现象**：透明 PNG 输出为 JPEG（不支持透明）时，透明区域变黑或 alpha 被丢。
- **修复**：`process_normal` 先判定源图 `has_alpha()`，JPEG 分支在编码前用 `flatten_rgba8_to_rgb` 按白底合成（alpha=255 时等价直取 RGB、无色偏）。PNG/WebP 输出天然保留 alpha，不受影响。

## AI 接口工程成熟度补齐

- **输出格式新增 WebP**：`--output-format webp`（CLI）/ `output_format:"webp"`（JSON）。更省体积、且支持透明通道。
- **`error_type` 细分**：失败时给出 `unsupported` / `corrupt` / `permission` / `skipped` / `passthrough` / `error`，便于 agent 决策"重试还是跳过"。
- **汇总新增 `skipped` 计数**：跳过（隐藏文件）与透传都不计入 `failed`。
- **新增 `manifest` 映射清单**：输入→输出逐项映射（含未压缩项），便于 agent 回映射源目录。
- **退出码语义固化**：0=正常（含隐藏文件跳过/透传），1=真正失败，2=参数错误。AI 脚本零歧义判断。

## 画质 / 体积（与 v4.3.0 一致，编码器未改）

编码器仍是 `mozjpeg-rs` 0.9.2（BSD-3，纯 Rust，零 C 依赖），v4.3.0 的相对提升全部保留：

| 维度 | v4.2.0 | v4.3.0/4.3.1 | 提升 |
|---|---|---|---|
| 同质量 Q95 不缩放体积 | 9530 KB | 5004 KB | **省 47.5%** |
| 同体积(~2MB) 画质 PSNR | 48.99 dB | 51.60 dB | **+2.6 dB（同大小更清晰）** |
| 同体积(~2MB) 画质 SSIM | 0.9929 | 0.9957 | 更高 |
| 元数据 | 会丢 ICC / XMP | EXIF + XMP + ICC + MPF 全保留 | 零损失 |

## 兼容性

- 不传任何新参数 = v4.1.0 / v4.3.0 旧行为完全不变；旧版 `config.toml` 也能正常加载（`serde(default)`）。
- GUI 锁死 v4.1.0 行为，感知压缩与所有 v4.3.1 工程参数仅对 CLI / AI-JSON 通路开放（与 v4.3.0 一致）。
- macOS / Windows 均原生编译，无外部运行时。

## 下载
- `图片高速压缩_Mac_v4.3.1.zip` — macOS .app
- `图片高速压缩_Win_v4.3.1.zip` — Windows exe
