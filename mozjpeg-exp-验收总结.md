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
| **ICC/EXIF/XMP 全段保留** | 调研发现 JPEG 的 ICC 实际存于 **APP2**（非 APP1）；v4.2.0 只取首个 APP1 会丢 XMP 与宽色域 ICC。v4.3.0 改为保留**所有 APP 段(0xE0–0xEF)**：EXIF+XMP+ICC+MPF 全部原样保留。经注入 ICC_PROFILE 的测试图实证：输出确实含 ICC_PROFILE（True） | 彻底修掉丢 ICC 坑 ✅ |
| **perceptual 量化表** | 改用内置 `QuantTableIdx::MssimTuned`（替代原 CSF 自定义表，mozjpeg-rs 不支持外部注入） | ✅ |
| **clippy** | `--release` 零警告（修 `as u32` 多余 cast 2 处） | 零警告 ✅ |
| **功能无回归** | archive 不缩放(_hd 原尺寸 8192×5464) / 大图 TIFF 串行 90KB / 旧 CLI 兼容 / JSON 文件参数 | 全过 ✅ |
| **双平台编译** | Mac release 零警告；Win `cargo xwin` 15MB exe 编译通过 | ✅ |

## 三、发版结果（用户选 A：合并 main → 发 4.3.0）

- ✅ 合并：本地 `main` 分支指向 4.3.0 合并点（含完整历史）
- ✅ GitHub：源码经 `gh api` 推送 main（21 文件全成功，v4.2.0 旧 git HTTPS 被代理阻断改 API 通道）
- ✅ Release：[v4.3.0](https://github.com/cscb603/StarTap-Image-Shrinking-Tool/releases/tag/v4.3.0) 已建，资产 `_Mac_v4.3.0.zip`(13.8MB) / `_Win_v4.3.0.zip`(8.2MB)
- ✅ 外置盘 + Downloads：双平台 zip 均已存入
- ✅ 版本号：Cargo.toml / build_mac_app.sh / gui `about_version` / Info.plist 均 4.3.0

## 四、已知遗留（非阻塞）

1. `color_space=ConvertToSRGB` 未实现，当前等价于 KeepOriginal（不转换）—— 本期暂缓，标注 TODO
2. `perceptual::csf_quant_tables()` 原函数定义变为死代码（不再被调用），保留备用
3. Win xwin 二进制仅编译验证，未实跑（纯 Rust 跨平台，风险低）

源文件改动：Cargo.toml + src/lib.rs + src/cli.rs + src/runner.rs + src/gui.rs + Info.plist + build_mac_app.sh（含版本号 + P4 ICC 修正）。
