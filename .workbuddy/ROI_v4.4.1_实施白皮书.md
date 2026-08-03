# 图片高速压缩 v4.4.1 — ROI 改进实施白皮书
> 路径：/Users/xtap/Documents/AI/星TAP-高清缩图-v4.2.0-perceptual-exp/.workbuddy/ROI_v4.4.1_实施白皮书.md
> 版本：v1.0 | 状态：批准待执行（智能执行模式，一路到底）| 依据：v4.4.1_UIUX_工业级审查.md §三/§四
> 实测环境：macOS 26 · rustc/cargo 1.93.1 · 工作树 main 与 origin/main 同步（ahead/behind=0）
> 总代码量：src/ 4482 行（3 文件）| 修改：~80 行 | 新增：1 常量 + 0 新文件

## §0 技能调用指南
| Phase 时机 | 调用 | 用途 |
|-----------|------|------|
| 改前 | sts-x / Read | 定位行号（已用 Grep 完成） |
| 每 Phase 后 | `cargo check --features "gui,cli"` | 编译门禁 |
| 全部完成后 | `cargo test --lib` | 13 测回归 |
| 收尾 | `cargo clippy --features "gui,cli" -- -D warnings` | 零警告 |

## §1 核心契约
- 一句话：给 GUI 工作线程加装 panic 兜底 + 输出目录自愈，并统一版本号与自定义模式画质归一化。
- 不做什么：不升版本号（App 仍 v4.4.1）、不 git push、不 Release、不碰压缩内核算法、不重构 lib.rs 配置映射。
- 约束：只改 `src/gui.rs` + `src/lib.rs` 的 1 个常量；禁止引入新依赖。

## §2 不做什么（代码级）
- ❌ 不改 `app_config_to_process_config` / `Processor` 内核逻辑
- ❌ 不新增文件（纯在现有文件内改）
- ❌ 不升 `Cargo.toml` 的 `version`（库 crate 0.1.1 与 App 产品版本 4.4.1 是两套体系，**不**用 `CARGO_PKG_VERSION`）
- ⚠️ 允许：新增 `pub const APP_VERSION` 作为唯一版本真源

## §2.5 后端→UI 映射
| 后端改动 | Phase | UI 落脚点 | 用户感知 |
|---------|-------|----------|---------|
| panic 捕获 | P0 | 工作线程崩溃不再卡「处理中」 | 异常时正常收尾，不假死 |
| 输出目录 create_dir_all | P0 | 自定义目录不存在时先建再写 | 不再因目录缺失整批失败 |
| quality_mode 归一化 | P1a | 切「自定义」卡片时 `quality_mode="normal"` | 模式切换不再显示错乱的「普通」 |
| APP_VERSION 常量 | P1b | 状态栏/首页标题/关于页/卡片 | 升版只改一处 |

## §3 成功标准（P0/P1）
| 级别 | 标准 | 量化 | 用户可见 |
|------|------|------|----------|
| P0 | 工作线程 `catch_unwind` 包裹，`panic` 后仍发 `ProcessingFinished` | UI 必脱离「处理中」 | 是 |
| P0 | 自定义输出目录 `fs::create_dir_all` 自愈 | 目录缺失时不再整批 Err | 是 |
| P1a | 切「自定义」卡片 `quality_mode`→`normal` | 自定义下拉首项匹配 | 是 |
| P1b | 3 处硬编码 `v4.4.1` → `APP_VERSION` 常量 | grep 仅 1 处定义 | 是 |
| 门禁 | `cargo check --features "gui,cli"` + `cargo test --lib` 13 测全绿 | 0 error | — |

## §4 发射前预检
- [x] 1. rustc/cargo 1.93.1 ✅  2. 磁盘充足 ✅  3. 源文件存在 ✅
- [x] 4. 当前编译状态：`cargo check --features "gui,cli"` 已知绿（审计 §2.2）
- [x] 5. 参考：v4.4.1_UIUX_工业级审查.md §三/§四
- [x] 6. 备份：git 工作树未提交改动已 commit（P0/P1 改动单独 commit）

## §5 文件组织
| 文件 | 改动 |
|------|------|
| `src/lib.rs` | L46 后新增 `pub const APP_VERSION: &str = "v4.4.1";` |
| `src/gui.rs` | L159 / L542 / L629 用 `APP_VERSION`；L246 线程 `catch_unwind` + 输出目录自愈；L709 切自定义归一化 `quality_mode` |

## §6 冲突矩阵
| 文件 | P0 | P1a | P1b | 风险 |
|------|----|-----|-----|------|
| `gui.rs` | L246 线程体 | L709 卡片 | L159/542/629 | 低（不同行）|
| `lib.rs` | — | — | L46 常量 | 低 |

执行铁律：每 Phase 改前 Read 当前行号；改后 `cargo check`；>2 次编译失败暂停。

## §8 实施顺序
### Phase 1（P1b 版本常量，最简单先落地）
1.1 `lib.rs` L46 后加 `pub const APP_VERSION: &str = "v4.4.1";`
1.2 `gui.rs` import 加 `APP_VERSION`；L159 `about_version: APP_VERSION.to_string()`
1.3 L542 / L629 改用 `format!("...{}", APP_VERSION)`
→ 验证：`cargo check --features "gui,cli"`

### Phase 2（P0 panic 捕获 + 输出目录自愈）
2.1 `gui.rs` L246 线程体：外移 `total`/`success_count` 到线程作用域；进入前 `fs::create_dir_all(custom_output_dir)`
2.2 用 `catch_unwind(AssertUnwindSafe(|| { 原处理循环 }))` 包裹；Err 时 `eprintln!("[ERROR] 工作线程 panic")`
2.3 循环结束后（无论是否 panic）发 `ProcessingFinished(total, success_count)`
→ 验证：`cargo check` + `cargo test --lib`

### Phase 3（P1a 自定义模式归一化）
3.1 `gui.rs` L709 切「自定义」卡片时追加 `self.config.quality_mode = "normal".to_string();`
→ 验证：`cargo check`

## §9 验收与停手线
- 运行：`cargo check --features "gui,cli" && cargo test --lib`
- **到此停止**：不升版本、不 git push、不 Release（发布已在 Phase C 完成）
- 本地 commit 保存进度，回主对话报告「验收 OK，P0/P1 已实现」

## §10 范围外（原定留待迭代 → 已于 2026-07-31 全量完成）
- P2 `FileItem.error` 写入 UI ✅ 已完成（见 §11）
- P2 拖拽 >1000 文件异步扫描 ✅ 已完成（见 §11）
- P3 GUI headless smoke test ✅ 已完成（见 §11）
- 原因（原）：ROI 抓大头，P0/P1 收益/风险比最高。用户后续要求 P0~P3 全做，故补充实施。

## 参考代码位置
| 要什么 | 去哪读 |
|--------|--------|
| 工作线程体 | `src/gui.rs:246-312` |
| AppEvent 枚举 | `src/gui.rs:35-44` |
| 自定义卡片点击 | `src/gui.rs:700-711` |
| 版本显示 3 处 | `src/gui.rs:159 / 542 / 629` |
| 配置映射 | `src/lib.rs:817-844` |

## §11 P2 / P3 补充实施记录（2026-07-31）

> 用户指令：p0～p3 都搞好，升级优化完善好哦，我后头来上传蓝奏云，再更新。
> 即：只改代码，不 push、不发版（用户自传蓝奏云并更新）。

### P2a 错误写入 + 文件列表 hover 展示
- `AppEvent::ProcessingProgress` 由 `(usize, usize)` 改为 `(usize, bool, Option<String>)`：携带失败原因。
- 工作线程：捕获 `processor.process_image` 的 `Err`，`result.err().map(|e| e.to_string())` 写入事件。
- 事件处理器 `ProcessingProgress` 分支：`item.error = err`，落盘到 `FileItem.error`。
- 新增文件列表渲染（处理完成后、`!self.processing` 时显示）：每行文件名 + ✅/❌/⏳ 状态图标；`item.error` 存在时 `on_hover_text` 显示「处理失败原因：…」。
- 此前 `FileItem.error` 字段存在但从未被读取；本次首次接入 UI。

### P2b 拖拽大量文件异步扫描
- 新增 `scanning: bool` 状态 + `scan_files_async()` / `flatten_paths()`（纯函数，递归展开目录）。
- 拖入与「浏览文件」改走 `scan_files_async`：标记 `scanning=true` → 后台线程递归收集 → `FilesAdded` 事件刷新列表并自动 `start_processing`。
- 拖放区新增「🔍 正在扫描文件…」态；拖放/点击处理期间 `!self.scanning` 门禁，防重入。
- 收益：>1000 文件场景不再在主线程卡顿。

### P3 GUI headless 序列化回归测试
- `src/gui.rs` 末尾新增 `#[cfg(test)] mod smoke_tests`：
  `config_custom_output_dir_roundtrip` —— 构造 `AppConfig { keep_original_name:true, custom_output_dir:Some(..) }`，`serde_json` 往返，断言字段保留且 JSON 含 `custom_output_dir`。
- 说明：eframe 无 headless 启动器（需 GPU/显示），「启动 app」环节由 `cargo check --features "gui,cli"`（编译整 GUI）+ 本契约共同守护；回归目标「config 序列化包含 custom_output_dir」已精确覆盖。
- 运行：`cargo test --bin rust_image_compressor --features "gui,cli"`（lib 13 测仍走 `cargo test --lib`）。

### 门禁结果（2026-07-31）
- `cargo check --features "gui,cli"` ✅
- `cargo test --lib`（13 测）✅
- `cargo test --bin rust_image_compressor --features "gui,cli"`（1 测）✅
- `cargo clippy --features "gui,cli" --all-targets -- -D warnings` ✅
- `cargo fmt --check` ✅
- 提交：本地 commit（未 push、未发版，用户自传蓝奏云更新）。
