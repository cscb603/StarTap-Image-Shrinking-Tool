# 星TAP 高清缩图 v4.3.0 编码器升级（mozjpeg-exp）白皮书
> 路径：/Users/xtap/Documents/AI/星TAP-高清缩图-v4.2.0-perceptual-exp/.workbuddy/mozjpeg-exp-蓝图.md
> 版本：v0.1 | 状态：标准模式（评审逐条确认）| 分支：mozjpeg-exp（基于 v4.2.0 稳定点 90124f3）
> 实测环境：macOS 26.5 / rustc 1.93.1 / cargo 1.93.1 / mozjpeg-rs 0.9.2(BSD-3) / 磁盘可用 44GB / 外置盘 295GB
> 总代码量：src/ ~840 行（cli.rs 42K / gui.rs 66K / lib.rs 47K / runner.rs 44K / perceptual.rs 26K）
> 上游调研：/Users/xtap/Downloads/ARLink/2026图像压缩升级方案.md（用户自研，已读）

## §0 技能调用指南（执行阶段按表调用）
| Phase 时机 | 调用技能 | 用途 |
|-----------|---------|------|
| 每个 Phase 完成后 | `code-reviewer` | 审查改动质量 |
| 改 encode 路径/量化表 | `rust-expert` | mozjpeg-rs API 用法确认 |
| 打包/交叉编译 | `rust-cross-platform-packaging` / `mac-app-packaging` | 出包避坑 |
| 遇 Bug | `systematic-debugging` | 分层定位，不盲试 |
| 代码定位 | `sts-x` | 替代 Grep/Read，省 ~80% token |
| 收尾 | `workspace-butler` | 清理临时文件 |

## §1 核心契约
- 一句话：用 `mozjpeg-rs` 替换 `jpeg-encoder`，照片默认 4:2:0 + 渐进 + optimize_huffman；修 ICC/EXIF 双 APP1 段保留 + 色彩空间。
- 不做什么：不接 JXL（AGPL 传染）/ 不接 AVIF（微信不支持）/ 不实现外部自定义量化表注入（mozjpeg-rs 不支持）。
- 成功标准：同质量体积 -20~40%；SSIM 不降；perceptual 模式改用内置 `MssimTuned` 表；ICC 与 EXIF 完整保留。
- 约束：BSD-3 许可（mozjpeg-rs）不影响项目许可；纯 Rust 零 C 依赖；Mac+Win 双平台验证。

## §2 不做什么（代码级）
- ❌ 不移除 `target_kb` 二分搜索逻辑（只改内部 encode 函数）
- ❌ 不改 GUI 版面（沿用 v4.2.0 三用途卡片）
- ❌ 不升版本号到 4.3.0 直到验收通过（本分支内部先用 4.3.0-exp 标记）
- ❌ 不 push / 不 Release / 不动外置盘（验收后由用户拍板合并/发版）
- ⚠️ 允许改：lib.rs encode 路径、preserve_exif_safe、config 新增 subsampling 字段、Cargo.toml 依赖

## §2.5 后端→UI 映射表
| 后端功能 | Phase | UI 落脚点 | 改动 | 用户感知 |
|---------|-------|----------|------|---------|
| subsampling 开关 | P2 | GUI 自定义折叠区新增「色彩子采样」ComboBox（照片4:2:0/截图4:4:4/平衡4:2:2）| 新增 callback | 截图文字可选 4:4:4 防文字模糊 |
| perceptual 量化表切换 | P3 | 无 UI（内部）| 用 MssimTuned 替代 CSF | 感知模式画质升级 |

## §3 成功标准（逐条验收）
| 级别 | 标准 | 量化指标 | 用户可见 |
|------|------|---------|---------|
| P0 | mozjpeg-rs 编码产出有效 JPEG | 同输入 `image::load_from_memory` 可解码 | 是（输出图正常） |
| P0 | 体积收益 | 同质量(Q95) 体积比 jpeg-encoder 小 ≥20% | 是（文件更小） |
| P0 | 不丢功能 | target_kb 二分 / EXIF / TIFF / 优雅停止 全通过 | 是 |
| P1 | SSIM 不降 | 同图 mozjpeg-rs vs jpeg-encoder SSIM 差 ≤0.01 | 否（指标） |
| P1 | ICC 双段保留 | 宽色域图输出含 ICC_PROFILE 段 | 是（专业用户可见） |
| P1 | perceptual 表 | 用 `QuantTableIdx::MssimTuned`，无自定义表报错 | 否 |
| P2 | subsampling 默认 | 照片路径 S420，截图 S444 | 是（选项） |

## §4 火箭发射前检查清单
### 4.1 环境实测
| 项目 | 实测值 | 要求 | 状态 |
|------|--------|------|------|
| rustc | 1.93.1 | ≥1.89 | ✅ |
| mozjpeg-rs | 0.9.2 | 最新 | ✅ |
| 磁盘 | 44GB | ≥10GB | ✅ |
| 外置盘 | 295GB | - | ✅ |
### 4.2 预检
[x] 1.环境版本 [x] 2.磁盘 [x] 3.源文件存在 [x] 4.当前编译状态(已 release 过) [x] 5.参考文件(mozjpeg-rs README) [x] 6.依赖(mozjpeg-rs crates.io 验证) [x] 7.分支已切 mozjpeg-exp

## §5 文件组织
- 新增：`subsampling` 配置字段（AppConfig + ProcessConfig + CLI/JSON）
- 修改：`Cargo.toml`(依赖) / `src/lib.rs`(encode 路径 + preserve_exif_safe + color_space) / `src/cli.rs`(参数) / `src/gui.rs`(ComboBox) / `src/runner.rs`(传参)
- 备份：改前 `git stash` 或 commit 基线（已在 90124f3）

## §6 Phase 冲突矩阵
| 文件 | P1(依赖+encode) | P3(量化表) | P4(EXIF双段) | P5(color_space) | 冲突 |
|------|----|----|----|----|------|
| `src/lib.rs` | 511-560 encode | 528-548 表选择 | 639-675 preserve | 418-440 转换 | ⚠️高（同文件多 Phase）|
| `Cargo.toml` | 依赖 | - | - | - | 低 |
### 6.2 铁律
1. Phase 严格按序，每 Phase 改前 Read 确认行号
2. 每 Phase 后 `cargo check` 通过才进下一
3. 编译失败 >2 次 → 暂停报告
4. encode 闭包 `encode_jpeg` 是 P1 核心，P3 只改其内部表选择分支

## §7 经验教训
### 7.1 已踩过的坑
- jpeg-encoder 0.7 默认 4:4:4（ColorType::Rgb）浪费 ~1/3 码率 → mozjpeg-rs S420 修
- preserve_exif_safe 只取第一个 0xE1 → 宽色域 ICC 丢失 → 改遍历双段
- csf_quant_tables 自定义表无法注入 mozjpeg-rs → 改用内置 MssimTuned
### 7.2 设计原则
- 编码器可替换：encode 函数抽象为闭包，未来换编码器只改一处
- ICC/EXIF 用 img-parts 后处理，与编码器解耦

## §8 实施顺序
### P1：依赖替换 + encode 重写（lib.rs:511-560）
1.1 Cargo.toml：移除 `jpeg-encoder`，加 `mozjpeg-rs = "0.9.2"`
1.2 lib.rs encode_jpeg 闭包：`jpeg_encoder::Encoder` → `mozjpeg_rs::Encoder::default().quality(q).progressive(true).optimize_huffman(true).subsampling(sub).encode_rgb(buf, w, h)`
1.3 验证：`cargo check` + 单图编码可解码
### P2：subsampling 参数（cli.rs/gui.rs/runner.rs/config）
2.1 AppConfig 加 `subsampling: String`（默认 "420"）
2.2 GUI 自定义区新增 ComboBox（照片420/截图444/平衡422）
2.3 encode 时映射 Subsampling 枚举
### P3：perceptual 量化表（lib.rs:528-548）
3.1 移除 `csf_quant_tables()` 自定义表调用
3.2 改为 `.qtable(QuantTableIdx::MssimTuned)`（perceptual 模式）；normal 模式用 `JpegAnnexK` 或默认
### P4：EXIF+ICC 双段（lib.rs:639-675）
4.1 preserve_exif_safe 遍历所有 0xE1，按签名区分 Exif\x00\x00 / ICC_PROFILE，都插入输出
### P5：色彩空间（lib.rs:418-440 + 输出后处理）
5.1 color_space=KeepOriginal：保留源 ICC 段（P4 已覆盖）
5.2 color_space=ConvertToSRGB：源图解码后转 sRGB（需 image crate 或 lcms2，本期若复杂则标记待确认，仅做 KeepOriginal）
### P6：验收（六项 + 双平台）
6.1 重跑 v4.2.0 六项验证 + 体积对比（mozjpeg-rs vs jpeg-encoder）
6.2 Mac release + Win xwin 编译
6.3 clippy 零警告

## §9 验收与交付边界
- 运行：`cargo fmt && cargo clippy --release`（零警告）+ 六项功能验证
- **停止线**：不升版本号发版 / 不 push / 不 Release / 不动外置盘。合并不合并由用户拍板（本分支独立验证，验收后另开对话讨论合并或独立发 4.3.0）。

## §10 自我处置 + 参考位置
- 参考：mozjpeg-rs README（github.com/imazen/mozjpeg-rs）| 当前 encode 路径 `src/lib.rs:511-560` | preserve_exif `src/lib.rs:639-675` | 调研 `2026图像压缩升级方案.md`
- 小问题记 TODO 继续；架构矛盾（如 mozjpeg-rs 不支持 ICC 嵌入）→ 用 img-parts 后处理绕过，不阻塞。
