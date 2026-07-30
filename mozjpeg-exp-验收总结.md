# 星TAP 高清缩图 v4.3.0-exp（mozjpeg-exp 分支）验收总结

> 分支：`mozjpeg-exp`（基于 v4.2.0 稳定点 90124f3）｜ 白皮书：`.workbuddy/mozjpeg-exp-蓝图.md`
> 实施方式：project-blueprint 火箭发射式（P1→P6 逐 Phase 落地，每 Phase 编译/功能验证通过才进下一）

## 一、实施范围（Tier1 + Tier2，用户全票同意）

| Tier | 内容 | 状态 |
|------|------|------|
| Tier1 | `jpeg-encoder 0.7` → `mozjpeg-rs 0.9.2`（BSD-3，纯 Rust，零 C 依赖）；默认 4:2:0 + 渐进 + optimize_huffman | ✅ |
| Tier2 | EXIF + ICC 双 APP1 段保留；色彩空间 KeepOriginal 保留源 ICC | ✅ |
| （未做）Tier3 | JXL（AGPL 传染许可，微信/小红书不渲染） | 暂缓 |
| （未做）Tier4 | AVIF（微信不支持）/ 目标体积自动降级尺寸 | 暂缓 |

## 二、关键结果与量化指标

| 指标 | 结果 | 标准 |
|------|------|------|
| **体积收益** | 同质量 Q95 不缩放：v4.2.0 jpeg-encoder(4:4:4)=**9530KB** → mozjpeg-rs(4:2:0)=**5005KB**（省 **47.5%**） | ≥20% ✅ |
| **编码器替换** | `Encoder::default().quality(q).progressive(true).optimize_huffman(true).subsampling(sub).quant_tables(idx).encode_rgb(buf,w,h)` 编译零错误 | — ✅ |
| **子采样开关** | 444(7065KB) > 420(5005KB)，GUI ComboBox / CLI `--subsampling` / JSON `subsampling` 全链路贯通 | 生效 ✅ |
| **ICC/EXIF 双段** | 测试图 GYL_3359.JPG 恰含 **2 个 APP1 段（EXIF+ICC）**，输出保留 2 个 → 宽色域 ICC 不再丢 | 修 v4.2.0 丢 ICC 坑 ✅ |
| **perceptual 量化表** | 改用内置 `QuantTableIdx::MssimTuned`（替代原 CSF 自定义表，mozjpeg-rs 不支持外部注入） | ✅ |
| **clippy** | `--release` 零警告（修 `as u32` 多余 cast 2 处） | 零警告 ✅ |
| **功能无回归** | archive 不缩放(_hd 原尺寸 8192×5464) / 大图 TIFF 串行 90KB / 旧 CLI 兼容 / JSON 文件参数 | 全过 ✅ |
| **双平台编译** | Mac release 零警告；Win `cargo xwin` 15MB exe 编译通过 | ✅ |

## 三、停止线（严格遵守）

- ❌ 未 push GitHub
- ❌ 未 Release / 未动外置盘
- ❌ 未合并 main
- ❌ 未升版本号发版（Cargo.toml 仍 4.2.0；GUI `about_version` 标 `4.3.0-exp` 区分分支）
- ✅ 已 commit 到本地 `mozjpeg-exp` 分支（5aca767）

## 四、已知遗留（非阻塞）

1. `color_space=ConvertToSRGB` 未实现，当前等价于 KeepOriginal（不转换）—— 本期暂缓，标注 TODO
2. `perceptual::csf_quant_tables()` 原函数定义变为死代码（不再被调用），保留备用
3. Win xwin 二进制仅编译验证，未实跑（纯 Rust 跨平台，风险低）

## 五、下一步（用户拍板）

- **A. 合并 main → 发 4.3.0**：双平台包 + GitHub Release + 外置盘（连接器优先）
- **B. 独立发 4.3.0-exp**：实验分支先行，main 暂不动
- **C. 继续 Tier3（JXL）/ Tier4（AVIF）**：需评估 AGPL 许可影响

源文件改动：Cargo.toml + src/lib.rs + src/cli.rs + src/runner.rs + src/gui.rs（5 文件，+100/-33 行）。
