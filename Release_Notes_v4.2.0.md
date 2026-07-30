# 星TAP 高清缩图 v4.2.0 — 拖进去就压好，三档用途一键选

## 一句话
朋友圈 / 小红书发图被平台压糊？v4.2.0 把压缩按「社交分享 / 高清存档 / 自定义」拆成三档，拖进去自动压到刚好平台不二次压缩的大小，画质还比之前更干净。

## 三个核心好处
- **社交分享**：自动按微信 / 小红书 / Instagram 卡线压（长边 + 体积双控），发出去还是你的高画质版，不再被平台糊掉。
- **高清存档**：原尺寸一像素不动，只压掉冗余体积 —— 124MB 中画幅 → 38MB，细节全留。
- **小而美感知压缩**：同样体积，主体更清晰、色彩更好（盲测 5 人里 4 人偏好新图）。

## 30 秒上手
- Mac：下载 `.app`，拖照片进去就压好，输出在同目录 `compressed/`。
- Win：下载 `.exe`，双击运行（零依赖，CRT 静态链接）。
- 高级：命令行 `--json` 给 AI / 脚本调用，支持 stdin 管道，不挂死。

## 这次升级了什么
- **三用途 GUI**：社交分享 / 高清存档 / 自定义，平台与画质下拉一键选。
- **三层解耦**：内核(lib) / GUI / CLI·AI 接口 分离，稳定可独立更新。
- **TIFF 支持**：拖入 `.tif` / `.tiff` 直接压（之前不识别）。
- **大图 OOM 加固**：超大图（>80MB / >4000 万像素）串行处理，拖一堆也不崩。
- **优雅停止**：处理中按停止，当前张处理完才停，已输出图不破坏。
- **EXIF 完整保留**：相机型号 / 参数信息原样带入输出图。
- **双平台原生包**：Mac `.app`（ad-hoc 签名）/ Win `.exe`（零依赖双击即用）。

---

# StarTap Image Shrinking Tool v4.2.0 — Drop in, compressed. Three modes, one tap.

## Highlights
- **Three-purpose GUI**: Social Share / HD Archive / Custom. Platform + quality dropdowns.
- **Layered architecture**: lib (engine) / GUI / CLI·AI-JSON separated for safe updates.
- **TIFF support**, **OOM guard** for huge images, **graceful stop**, **full EXIF kept**.
- **Perceptual compression** (CSF quantization): same size, cleaner subject & color.
- **Dual-platform**: macOS `.app` (signed) / Windows `.exe` (CRT static-linked, zero-dep).

## Assets
- `图片高速压缩_Mac_v4.2.0.zip` — macOS .app
- `图片高速压缩_Win_v4.2.0.zip` — Windows exe
