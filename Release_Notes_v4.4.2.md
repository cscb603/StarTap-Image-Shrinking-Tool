# 星TAP 高清缩图 v4.4.2 — 稳定性加固 + 失败可见 + 大批量不卡

v4.4.1 把"导出目录记忆"做顺了。v4.4.2 回到**健壮性和体验细节**：单张图崩溃不再拖垮整个软件、失败原因看得到、一次拖上千张也不卡界面。

## 一句话功能

**某张图炸了只标红这一张，其余继续跑；失败原因悬停即见；批量扫描搬后台，界面始终跟手。**

## 这次改了什么

### P0 · 稳定性加固（最高优先级）

- **工作线程 panic 兜底**：每张图处理包在 `catch_unwind` 里，单张解码/编码 panic 不再让整个 GUI 卡死或闪退；失败计入"失败数"，处理完照常出清单。
- **输出目录自愈**：自定义导出目录不存在时自动 `create_dir_all` 建好，不再因目录缺失而写不进。
- **版本号唯一真源**：新增 `APP_VERSION` 常量，关于页 / 状态栏 / 卡片标题三处统一引用，升版只改一处，杜绝"一处新版一处旧版"。
- **自定义模式画质归一化**：切「自定义」卡片时 `quality_mode` 归一到 `normal`，避免历史脏值导致画质档位错乱。

### P2a · 失败可见

- `ProcessingProgress` 协议由 `(index, ok)` 升级为 `(index, ok, Option<错误文本>)`。
- 处理完新增「📋 文件列表」：✅ 成功 / ❌ 失败 / ⏳ 处理中；**失败项悬停显示具体失败原因**（如"文件不存在或无法解码"）。
- 之前 `FileItem.error` 字段一直空置、从未被读取，本次正式接入 UI。

### P2b · 大批量不卡

- 新增 `scanning` 状态 + 后台扫描线程（`scan_files_async` / `flatten_paths` 纯函数递归展开目录）。
- 拖入与「浏览文件」改走异步扫描：拖放区显示「🔍 正在扫描文件…」，扫完经 `FilesAdded` 事件刷新列表并自动开始，主线程全程不阻塞。**>1000 文件场景不再卡顿**。

### P3 · 防回归测试

- `src/gui.rs` 新增 `#[cfg(test)]` 配置序列化往返回归测试：`AppConfig { keep_original_name, custom_output_dir }` 经 `serde_json` 往返后字段保留、JSON 含 `custom_output_dir`。
- 长期守护"自定义导出目录配置不被改丢"这类隐性回归。

## 兼容性

- v4.4.0 / v4.4.1 所有功能（画质优先、CAS 锐化、平台预设、导出目录记忆）全量保持。
- 配置文件向后兼容；CLI/JSON 行为不变，AI 调用方不受影响。

## 门禁

`cargo check --features "gui,cli"` ✅ · lib 13 测 ✅ · GUI 1 测 ✅ · `clippy -D warnings` ✅ · `cargo fmt` ✅

## 下载

- 🍎 Mac：https://wwbfk.lanzoub.com/ifuNW40bu62j
- 🪟 Win：https://wwbfk.lanzoub.com/iQxvR40bu5ij
- GitHub Release（备用）：https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases/tag/v4.4.2
