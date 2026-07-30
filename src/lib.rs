pub mod perceptual;

use anyhow::Result;
use bytes::Bytes;
use fast_image_resize as fr;
use image::GenericImageView;
use img_parts::jpeg::Jpeg;
use memmap2::Mmap;
use perceptual::{FocusMode, PerceptualMetrics, PerceptualOptions};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicUsize, Ordering};

// 摄影级优化导入
use image::ImageBuffer;
use image::Rgba;

#[cfg(target_os = "macos")]
static SIPS_CONCURRENCY: AtomicUsize = AtomicUsize::new(0);
#[cfg(target_os = "macos")]
const MAX_SIPS_CONCURRENCY: usize = 4;

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum ProcessMode {
    WeChat,
    HD,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum OutputFormat {
    Jpeg,
    KeepOriginal,
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub enum ColorSpace {
    KeepOriginal,
    ConvertToSRGB,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppConfig {
    pub mode: ProcessMode,
    pub custom_max_dim: u32,
    pub custom_quality: u8,
    pub custom_target_kb: u32,
    pub overwrite: bool,
    pub keep_original_name: bool,
    pub output_format: OutputFormat,
    pub color_space: ColorSpace,
    // 摄影级优化选项
    pub enable_sharpening: bool,       // USM 锐化
    pub sharpening_radius: f32,        // 锐化半径 (默认 1.0)
    pub sharpening_amount: f32,        // 锐化强度 (默认 0.8)
    pub use_custom_quantization: bool, // 自定义量化表
    pub preserve_high_frequency: bool, // 保留高频细节
    // v4.2.0：用途 / 平台 / 画质模式（GUI 记忆 + 三方接口对齐）
    // usage_mode: "social"(社交分享) | "archive"(高清存档) | "custom"(自定义)
    // quality_mode: "perceptual"(小而美感知压缩) | "normal"(普通标准压缩)
    // platform: wechat/wechat-new/xiaohongshu/instagram/general
    // serde(default)：旧版 config.toml 没有这三个字段，升级后仍能正常加载（配置记忆不丢）
    #[serde(default = "default_usage_mode")]
    pub usage_mode: String,
    #[serde(default = "default_quality_mode")]
    pub quality_mode: String,
    #[serde(default = "default_platform")]
    pub platform: String,
}

fn default_usage_mode() -> String {
    "social".to_string()
}
fn default_quality_mode() -> String {
    "perceptual".to_string()
}
fn default_platform() -> String {
    "wechat".to_string()
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: ProcessMode::Custom,
            custom_max_dim: 3000,
            custom_quality: 85,
            custom_target_kb: 0,
            overwrite: false,
            keep_original_name: false,
            output_format: OutputFormat::Jpeg,
            color_space: ColorSpace::KeepOriginal,
            // 摄影级优化默认关闭（保持原有行为）
            enable_sharpening: false,
            sharpening_radius: 1.0,
            sharpening_amount: 0.8,
            use_custom_quantization: false,
            preserve_high_frequency: false,
            // 默认社交分享 + 小而美感知压缩 + 微信平台（最常用组合）
            usage_mode: "social".to_string(),
            quality_mode: "perceptual".to_string(),
            platform: "wechat".to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ProcessConfig {
    pub mode: ProcessMode,
    pub max_dim: u32,
    pub quality: u8,
    pub target_kb: u32,
    pub output_dir: Option<PathBuf>,
    pub overwrite: bool,
    pub keep_original_name: bool,
    pub output_format: OutputFormat,
    pub color_space: ColorSpace,
    // 摄影级优化
    pub enable_sharpening: bool,
    pub sharpening_radius: f32,
    pub sharpening_amount: f32,
    /// v4.2.0-exp 感知压缩选项：None = 完全走 v4.1.0 旧路径（GUI 恒为 None）
    pub perceptual: Option<PerceptualOptions>,
}

pub struct Processor {
    config: ProcessConfig,
}

impl Processor {
    pub fn new(config: ProcessConfig) -> Self {
        Self { config }
    }

    /// 纯路径计算：给定输入文件，返回将要生成的输出路径（不创建目录、不做任何 IO 写入）。
    /// 供上层「续跑跳过」幂等判断复用，与 process_image 内部逻辑严格同源。
    pub fn expected_output_path(&self, input_path: &Path) -> PathBuf {
        let healed_path = path_self_healing(input_path);
        let file_stem = healed_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let extension = healed_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let suffix = if self.config.mode == ProcessMode::WeChat {
            "_wx"
        } else if self.config.mode == ProcessMode::HD {
            "_hd"
        } else {
            "_da"
        };

        let output_dir = self
            .config
            .output_dir
            .clone()
            .unwrap_or_else(|| healed_path.parent().unwrap_or(Path::new(".")).to_path_buf());

        let output_ext = match self.config.output_format {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::KeepOriginal => match extension.as_str() {
                "png" => "png",
                _ => "jpg",
            },
        };

        if self.config.overwrite {
            healed_path.to_path_buf()
        } else if self.config.keep_original_name {
            output_dir.join(format!("{}.{}", file_stem, output_ext))
        } else {
            output_dir.join(format!("{}{}.{}", file_stem, suffix, output_ext))
        }
    }

    pub fn process_image(&self, input_path: &Path) -> Result<PathBuf> {
        self.process_image_with_metrics(input_path).map(|(p, _)| p)
    }

    /// v4.2.0-exp：处理并返回感知指标（perceptual=None 时指标恒为 None，行为同 v4.1.0）
    pub fn process_image_with_metrics(
        &self,
        input_path: &Path,
    ) -> Result<(PathBuf, Option<PerceptualMetrics>)> {
        let healed_path = path_self_healing(input_path);
        let file_name_os = healed_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if file_name_os.starts_with("._") {
            return Err(anyhow::anyhow!("跳过系统隐藏文件"));
        }

        let extension = healed_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let raw_extensions = [
            "dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw", "3fr",
        ];
        let is_raw = raw_extensions.contains(&extension.as_str());

        let output_dir = self
            .config
            .output_dir
            .clone()
            .unwrap_or_else(|| healed_path.parent().unwrap_or(Path::new(".")).to_path_buf());
        if !output_dir.exists() {
            fs::create_dir_all(&output_dir)?;
        }

        // 输出路径与 expected_output_path 严格同源
        let output_path = self.expected_output_path(&healed_path);

        let metrics: Option<PerceptualMetrics>;

        #[cfg(target_os = "macos")]
        {
            let file_stem = healed_path.file_stem().unwrap().to_string_lossy();
            if is_raw {
                self.process_raw(&healed_path, &output_path, &file_stem, &file_name_os)?;
                metrics = None;
            } else {
                metrics = self.process_normal(&healed_path, &output_path, &extension)?;
            }
        }

        #[cfg(not(target_os = "macos"))]
        if is_raw {
            return Err(anyhow::anyhow!(
                "RAW 格式 ({}) 仅在 macOS 系统上支持。请先在 Mac 上处理,或转成 JPG/PNG 后使用。",
                extension
            ));
        } else {
            metrics = self.process_normal(&healed_path, &output_path, &extension)?;
        }

        Ok((output_path, metrics))
    }

    /// 返回当前感知压缩配置（None = 走 v4.1.0 旧路径），供 CLI/JSON 输出 metrics 元数据
    pub fn perceptual_config(&self) -> Option<&PerceptualOptions> {
        self.config.perceptual.as_ref()
    }

    /// 实际生效的体积预算（KB）：感知模式 budget_kb 覆盖 target_kb
    pub fn effective_target_kb(&self) -> u32 {
        match &self.config.perceptual {
            Some(p) => p.budget_kb.unwrap_or(self.config.target_kb),
            None => self.config.target_kb,
        }
    }

    #[cfg(target_os = "macos")]
    fn process_raw(
        &self,
        input_path: &Path,
        output_path: &Path,
        file_stem: &str,
        file_name: &str,
    ) -> Result<()> {
        while SIPS_CONCURRENCY.load(Ordering::SeqCst) >= MAX_SIPS_CONCURRENCY {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        SIPS_CONCURRENCY.fetch_add(1, Ordering::SeqCst);

        let input_path_abs =
            fs::canonicalize(input_path).unwrap_or_else(|_| input_path.to_path_buf());
        let result = (|| -> Result<()> {
            let mut preview_cmd = std::process::Command::new("sips");
            preview_cmd
                .arg("-e")
                .arg("preview")
                .arg(&input_path_abs)
                .arg("--out")
                .arg(output_path);

            let _ = preview_cmd.output();
            if output_path.exists() {
                // max_dim == 0 表示「不缩放」（高清存档），跳过 sips -Z
                if self.config.max_dim > 0 {
                    if let Ok(img) = image::open(output_path) {
                        let (w, h) = img.dimensions();
                        if w > self.config.max_dim || h > self.config.max_dim {
                            let mut resize_cmd = std::process::Command::new("sips");
                            resize_cmd.arg("-Z").arg(self.config.max_dim.to_string());
                            resize_cmd.arg(output_path);
                            let _ = resize_cmd.output();
                        }
                    }
                }
                return Ok(());
            }

            let mut cmd = std::process::Command::new("sips");
            cmd.arg("-s").arg("format").arg("jpeg");
            let quality = self.config.quality;
            cmd.arg("-s").arg("formatOptions").arg(quality.to_string());
            if self.config.max_dim > 0 {
                cmd.arg("-Z").arg(self.config.max_dim.to_string());
            }
            cmd.arg(&input_path_abs).arg("--out").arg(output_path);

            let mut child = cmd.spawn()?;
            let timeout = std::time::Duration::from_secs(30);
            let start = std::time::Instant::now();

            let status = loop {
                match child.try_wait()? {
                    Some(status) => break status,
                    None => {
                        if start.elapsed() > timeout {
                            let _ = child.kill();
                            break std::process::ExitStatus::default();
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }
            };

            if status.success() && output_path.exists() {
                return Ok(());
            }

            let temp_dir = std::env::temp_dir().join("rust_compressor_ql");
            let _ = fs::create_dir_all(&temp_dir);
            let mut ql_cmd = std::process::Command::new("qlmanage");
            ql_cmd.arg("-t").arg("-s");
            // max_dim == 0（不缩放）时 qlmanage 仍需尺寸参数 → 给超大值等效原尺寸
            if self.config.max_dim > 0 {
                ql_cmd.arg(self.config.max_dim.to_string());
            } else {
                ql_cmd.arg("20000");
            }
            ql_cmd.arg("-o").arg(&temp_dir).arg(&input_path_abs);

            if let Ok(mut child) = ql_cmd.spawn() {
                let start = std::time::Instant::now();
                loop {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                    if start.elapsed().as_secs() > 30 {
                        let _ = child.kill();
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
            }

            let ql_file_1 = temp_dir.join(format!("{}.png", file_stem));
            let ql_file_2 = temp_dir.join(format!("{}.png", file_name));
            let ql_file = if ql_file_1.exists() {
                Some(ql_file_1)
            } else if ql_file_2.exists() {
                Some(ql_file_2)
            } else {
                None
            };

            if let Some(path) = ql_file {
                let mut conv_cmd = std::process::Command::new("sips");
                conv_cmd
                    .arg("-s")
                    .arg("format")
                    .arg("jpeg")
                    .arg(&path)
                    .arg("--out")
                    .arg(output_path);
                let _ = conv_cmd.output();
                let _ = fs::remove_file(path);
                if output_path.exists() {
                    return Ok(());
                }
            }

            Err(anyhow::anyhow!("该机型 RAW 暂不支持"))
        })();

        SIPS_CONCURRENCY.fetch_sub(1, Ordering::SeqCst);
        result?;
        Ok(())
    }

    fn process_normal(
        &self,
        input_path: &Path,
        output_path: &Path,
        extension: &str,
    ) -> Result<Option<PerceptualMetrics>> {
        let img = load_image_safe(input_path)?;
        let (width, height) = img.dimensions();
        let perceptual = self.config.perceptual.as_ref();
        let mut pm = PerceptualMetrics::default();

        // max_dim == 0 表示「不缩放，保持原图尺寸」（高清存档/投稿场景）
        let scale = if self.config.max_dim > 0
            && (width > self.config.max_dim || height > self.config.max_dim)
        {
            let ratio_w = self.config.max_dim as f32 / width as f32;
            let ratio_h = self.config.max_dim as f32 / height as f32;
            ratio_w.min(ratio_h)
        } else {
            1.0
        };

        let new_width = (width as f32 * scale) as u32;
        let new_height = (height as f32 * scale) as u32;

        let img_rgba = img.to_rgba8();

        let t_down = std::time::Instant::now();
        let src_image = fr::images::Image::from_vec_u8(
            width,
            height,
            img_rgba.into_raw(),
            fr::PixelType::U8x4,
        )?;

        let mut dst_image = fr::images::Image::new(new_width, new_height, fr::PixelType::U8x4);

        let mut resizer = fr::Resizer::new();
        resizer.resize(&src_image, &mut dst_image, None)?;

        let rgba_buf = dst_image.buffer();
        pm.downscale_ms = t_down.elapsed().as_millis() as u64;

        // 转换为 DynamicImage 以便后续处理
        let mut dynamic_img = image::DynamicImage::ImageRgba8(
            image::ImageBuffer::from_raw(new_width, new_height, rgba_buf.to_vec())
                .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?,
        );

        // 感知指标参考帧：降采样后、锐化编码前的灰度图
        let reference_gray = perceptual.map(|_| perceptual::to_gray(&dynamic_img));

        if let Some(p) = perceptual {
            // 降噪在「目标分辨率」上做（§3 防御性修正：超大图全分辨率降噪会卡死/爆内存，
            // 统一放降采样后；高质量重采样已抑制高频噪点，目标分辨率降噪足以满足分享/存档画质）
            // JPG 输入禁降噪（放大块效应）；超过 4000px 的超大图直接跳过降噪（防冻结）
            let is_jpeg_input = matches!(extension, "jpg" | "jpeg");
            if p.denoise_strength > 0 && !is_jpeg_input && new_width.max(new_height) <= 4000 {
                let t = std::time::Instant::now();
                dynamic_img = image::DynamicImage::ImageRgba8(perceptual::bilateral_denoise(
                    &dynamic_img.to_rgba8(),
                    p.denoise_strength,
                ));
                pm.denoise_ms = t.elapsed().as_millis() as u64;
                pm.denoise_applied = true;
            }
            // 锐化在目标分辨率上做、且是编码前最后一步
            let t = std::time::Instant::now();
            let rgba_now = dynamic_img.to_rgba8();
            let mask = match p.focus_mode {
                FocusMode::Auto => perceptual::saliency_mask(&rgba_now),
                FocusMode::Center => perceptual::center_mask(new_width, new_height),
            };
            let larger = new_width.max(new_height);
            // 保守参数（§9 保画质铁律）：radius 随尺寸微调，amount 经掩码加权后有效强度更低
            let (radius, amount, threshold) = if larger < 1500 {
                (0.7, 0.55, 6)
            } else if larger < 3000 {
                (0.9, 0.7, 5)
            } else {
                (1.1, 0.8, 4)
            };
            dynamic_img =
                perceptual::masked_usm_sharpen(&dynamic_img, &mask, radius, amount, threshold);
            pm.sharpen_ms = t.elapsed().as_millis() as u64;
        } else if self.config.enable_sharpening {
            // v4.1.0 旧路径：智能自适应锐化（行为不变）
            dynamic_img = smart_adaptive_sharpen(&dynamic_img, new_width.max(new_height));
        }

        let mut result_data;
        use std::io::Cursor;

        let output_ext = match self.config.output_format {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::KeepOriginal => match extension {
                "png" => "png",
                _ => "jpg",
            },
        };

        match output_ext {
            "png" => {
                use image::codecs::png::{CompressionType, FilterType, PngEncoder};
                let mut cursor = Cursor::new(Vec::new());
                let encoder = PngEncoder::new_with_quality(
                    &mut cursor,
                    CompressionType::Best,
                    FilterType::Adaptive,
                );
                dynamic_img.write_with_encoder(encoder)?;
                result_data = cursor.into_inner();
            }
            "webp" => {
                let mut cursor = Cursor::new(Vec::new());
                dynamic_img.write_to(&mut cursor, image::ImageFormat::WebP)?;
                result_data = cursor.into_inner();
            }
            _ => {
                let t_encode = std::time::Instant::now();
                // 转换为 RGB 格式
                let rgb_img = dynamic_img.to_rgb8();
                let rgb_buf = rgb_img.as_raw();

                // 感知模式：budget_kb 覆盖 target_kb；质量上限 quality_ceil
                let effective_target_kb = match perceptual {
                    Some(p) => p.budget_kb.unwrap_or(self.config.target_kb),
                    None => self.config.target_kb,
                };
                let limit_bytes = if effective_target_kb > 0 {
                    Some((effective_target_kb as usize) * 1024)
                } else {
                    None
                };

                let quant_mode = perceptual.map(|p| p.quant_mode);
                let encode_jpeg = |quality: u8| -> Result<Vec<u8>, anyhow::Error> {
                    let mut buf = Vec::new();
                    let mut encoder = jpeg_encoder::Encoder::new(&mut buf, quality);
                    match quant_mode {
                        Some(perceptual::QuantMode::MsSsim) => {
                            // P2-A：jpeg-encoder 内置 MS-SSIM 调优表（走 quality 缩放，安全）
                            encoder.set_quantization_tables(
                                jpeg_encoder::QuantizationTableType::CustomMsSsim,
                                jpeg_encoder::QuantizationTableType::CustomMsSsim,
                            );
                        }
                        Some(perceptual::QuantMode::Csf) => {
                            // P2-B：自算 CSF 感知表。坑（§11.2-6）：Custom 表原值直用、
                            // 不做 quality 缩放 → 必须在生成时按 quality 自行缩放
                            let (luma, chroma) = perceptual::csf_quant_tables(quality);
                            encoder.set_quantization_tables(
                                jpeg_encoder::QuantizationTableType::Custom(Box::new(luma)),
                                jpeg_encoder::QuantizationTableType::Custom(Box::new(chroma)),
                            );
                        }
                        _ => {} // Standard / 非感知模式：v4.1.0 默认 Annex-K 表
                    }
                    encoder
                        .encode(
                            &rgb_buf[..],
                            new_width as u16,
                            new_height as u16,
                            jpeg_encoder::ColorType::Rgb,
                        )
                        .map_err(|e| anyhow::anyhow!("JPEG encoding failed: {}", e))?;
                    Ok(buf)
                };

                if let Some(limit) = limit_bytes {
                    let current_q = match perceptual {
                        Some(p) => self.config.quality.min(p.quality_ceil),
                        None => self.config.quality,
                    };
                    pm.final_quality = current_q;
                    let data = encode_jpeg(current_q)?;

                    if data.len() <= limit {
                        result_data = data;
                    } else {
                        let mut low = 1;
                        let mut high = current_q - 1;
                        let mut best_data = Vec::new();

                        while low <= high {
                            let mid = (low + high) / 2;
                            if mid == 0 {
                                break;
                            }

                            if let Ok(data) = encode_jpeg(mid) {
                                if data.len() <= limit {
                                    best_data = data;
                                    pm.final_quality = mid;
                                    low = mid + 1;
                                } else {
                                    if mid == 0 {
                                        break;
                                    }
                                    high = mid - 1;
                                }
                            } else {
                                break;
                            }
                        }

                        if !best_data.is_empty() {
                            result_data = best_data;
                        } else {
                            result_data = encode_jpeg(1)?;
                            pm.final_quality = 1;
                        }
                    }
                } else {
                    let q = match perceptual {
                        Some(p) => self.config.quality.min(p.quality_ceil),
                        None => self.config.quality,
                    };
                    pm.final_quality = q;
                    result_data = encode_jpeg(q)?;
                }
                pm.encode_ms = t_encode.elapsed().as_millis() as u64;

                // 感知指标：解码输出 JPG，与「降采样后参考帧」算 SSIM/PSNR
                if let (Some(_), Some((ref_gray, gw, gh))) = (perceptual, reference_gray.as_ref()) {
                    if let Ok(decoded) = image::load_from_memory(&result_data) {
                        let (out_gray, ow, oh) = perceptual::to_gray(&decoded);
                        if ow == *gw && oh == *gh {
                            pm.ssim_vs_source =
                                perceptual::ssim_gray(ref_gray, &out_gray, *gw, *gh);
                            pm.psnr_vs_source = perceptual::psnr_gray(ref_gray, &out_gray);
                        }
                    }
                }

                if extension == "jpg" || extension == "jpeg" {
                    result_data = preserve_exif_safe(input_path, &result_data);
                }
            }
        }

        fs::write(output_path, result_data)?;
        Ok(perceptual.map(|_| pm))
    }
}

fn preserve_exif_safe(input_path: &Path, result_data: &[u8]) -> Vec<u8> {
    let input_file = match fs::File::open(input_path) {
        Ok(file) => file,
        Err(_) => return result_data.to_vec(),
    };

    let input_mmap = match unsafe { Mmap::map(&input_file) } {
        Ok(mmap) => mmap,
        Err(_) => return result_data.to_vec(),
    };

    if input_mmap.len() > 100 * 1024 * 1024 {
        return result_data.to_vec();
    }

    let input_jpeg = match Jpeg::from_bytes(Bytes::copy_from_slice(&input_mmap)) {
        Ok(jpeg) => jpeg,
        Err(_) => return result_data.to_vec(),
    };

    let exif_segment = match input_jpeg.segments().iter().find(|s| s.marker() == 0xE1) {
        Some(seg) => seg.clone(),
        None => return result_data.to_vec(),
    };

    drop(input_mmap);
    drop(input_file);

    let output_jpeg = match Jpeg::from_bytes(Bytes::copy_from_slice(result_data)) {
        Ok(jpeg) => jpeg,
        Err(_) => return result_data.to_vec(),
    };

    let mut output_jpeg = output_jpeg;
    output_jpeg.segments_mut().insert(1, exif_segment);
    output_jpeg.encoder().bytes().to_vec()
}

/// 用途驱动的三方统一配置构造（GUI / CLI / AI JSON 同一套语义）
///
/// - social：平台预设值（已写入 `custom_*`）→ 社交分享最优体积/画质
/// - archive：不缩放(0) + 最高画质(100) + 不限位(0) → 高清存档
/// - custom：完全按用户自定义参数（含 `--mode hd` 的 4096/95/5000 兼容）
///
/// 平台预设 / 不缩放策略 / sRGB 强制 已在调用方（`apply_usage_preset` / `to_app_config` /
/// json 路径）写入 `config.custom_*` 与 `config.color_space`，本函数只负责按 `usage_mode` 选材。
/// 感知压缩（perceptual）由 `quality_mode` 决定，调用方在返回后按需覆盖。
pub fn app_config_to_process_config(
    config: &AppConfig,
    output_dir: Option<PathBuf>,
) -> ProcessConfig {
    let (mode, max_dim, quality, target_kb) = match config.usage_mode.as_str() {
        "social" => (
            ProcessMode::WeChat,
            config.custom_max_dim,
            config.custom_quality,
            config.custom_target_kb,
        ),
        // archive：不缩放(0) + 视觉无损画质(95，同原版 HD) + 不限体积(0)；ProcessMode::HD 仅决定输出后缀 "_hd"
        // 注意：不用 Q100 —— Q100 禁用量化几乎无压缩，输出会比源图更大，违背压缩工具语义
        "archive" => (ProcessMode::HD, 0, 95, 0),
        _ => {
            // custom：完整保留 v4.1.0 三种旧模式语义（向后兼容铁律）
            match config.mode {
                ProcessMode::HD => (ProcessMode::HD, 4096, 95, 5000),
                ProcessMode::WeChat => (ProcessMode::WeChat, 2048, 95, 900),
                ProcessMode::Custom => (
                    ProcessMode::Custom,
                    config.custom_max_dim,
                    config.custom_quality,
                    config.custom_target_kb,
                ),
            }
        }
    };
    ProcessConfig {
        mode,
        max_dim,
        quality,
        target_kb,
        output_dir,
        overwrite: config.overwrite,
        keep_original_name: config.keep_original_name,
        output_format: config.output_format,
        color_space: config.color_space,
        // 摄影级优化
        enable_sharpening: config.enable_sharpening,
        sharpening_radius: config.sharpening_radius,
        sharpening_amount: config.sharpening_amount,
        perceptual: None,
    }
}

// ============================================================================
// 摄影级优化功能 - 智能自适应锐化
// ============================================================================

/// 检测像素是否为肤色（快速算法）
///
/// 使用简单但有效的肤色检测算法
/// 返回值：0.0-1.0，值越高越可能是肤色
pub(crate) fn is_skin_color(r: u8, g: u8, b: u8) -> f32 {
    // RGB 转 YCbCr 色彩空间
    let _y = 0.299 * r as f32 + 0.587 * g as f32 + 0.114 * b as f32;
    let cb = -0.1687 * r as f32 - 0.3313 * g as f32 + 0.5 * b as f32 + 128.0;
    let cr = 0.5 * r as f32 - 0.4187 * g as f32 - 0.0813 * b as f32 + 128.0;

    // 肤色范围（经验值）
    let cb_min = 77.0;
    let cb_max = 127.0;
    let cr_min = 133.0;
    let cr_max = 173.0;

    if cb >= cb_min && cb <= cb_max && cr >= cr_min && cr <= cr_max {
        // 在肤色范围内
        let cb_score = 1.0 - ((cb - (cb_min + cb_max) / 2.0) / ((cb_max - cb_min) / 2.0)).abs();
        let cr_score = 1.0 - ((cr - (cr_min + cr_max) / 2.0) / ((cr_max - cr_min) / 2.0)).abs();
        (cb_score * cr_score).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// 检测图像中肤色区域比例
///
/// 返回值：0.0-1.0，表示图像中肤色区域的比例
pub(crate) fn estimate_skin_ratio(image: &image::DynamicImage) -> f32 {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    // 采样计算，避免全图处理影响性能
    let sample_step = 20;
    let mut skin_count = 0;
    let mut total_count = 0;

    for y in (0..height).step_by(sample_step as usize) {
        for x in (0..width).step_by(sample_step as usize) {
            let pixel = rgba.get_pixel(x, y);
            let skin_score = is_skin_color(pixel[0], pixel[1], pixel[2]);
            if skin_score > 0.3 {
                skin_count += 1;
            }
            total_count += 1;
        }
    }

    if total_count == 0 {
        return 0.0;
    }

    skin_count as f32 / total_count as f32
}

/// 智能锐化决策：根据图像特征判断是否需要锐化
///
/// 避免重复锐化的策略：
/// 1. 检测高频能量（已锐化的图高频能量高）
/// 2. 检测平坦区域比例（磨皮人像平坦区域多）
/// 3. 检测噪点水平（高噪点图不锐化）
/// 4. 根据尺寸和内容类型决策
fn should_apply_sharpening(image: &image::DynamicImage, _max_dim: u32) -> bool {
    let (width, height) = image.dimensions();
    let larger_dim = width.max(height);

    // 小图不锐化（避免过度处理）
    if larger_dim < 800 {
        return false;
    }

    // 检测肤色比例
    let skin_ratio = estimate_skin_ratio(image);

    // 如果肤色比例较高（> 30%），很可能是人像照片
    if skin_ratio > 0.3 {
        // 人像照片：降低锐化需求，避免锐化肤色
        return estimate_image_complexity(image) > 0.5;
    }

    // 超大图才需要明显锐化
    if larger_dim < 2000 {
        // 中等尺寸：仅当图像内容需要时才锐化
        return estimate_image_complexity(image) > 0.3;
    }

    // 大图：默认锐化，除非检测到问题
    if larger_dim >= 2000 {
        // 检查是否已足够锐利
        if is_already_sharp_enough(image) {
            return false;
        }

        // 检查是否高噪点（避免锐化噪点）
        if estimate_noise_level(image) > 0.6 {
            return false;
        }

        return true;
    }

    true
}

/// 估算图像复杂度（0.0-1.0）
/// 简单场景（天空、纯色背景）返回低值，复杂场景（纹理、细节）返回高值
fn estimate_image_complexity(image: &image::DynamicImage) -> f32 {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    // 采样 100x100 网格快速估算
    let sample_w = (width / 100).max(1);
    let sample_h = (height / 100).max(1);

    let mut complexity_sum = 0.0f32;
    let mut count = 0;

    for y in (0..height).step_by(sample_h as usize) {
        for x in (0..width).step_by(sample_w as usize) {
            let pixel = rgba.get_pixel(x, y);

            // 计算局部方差（简单版）
            let mut neighbors = Vec::new();
            if x > 0 {
                neighbors.push(rgba.get_pixel(x - 1, y));
            }
            if x < width - 1 {
                neighbors.push(rgba.get_pixel(x + 1, y));
            }
            if y > 0 {
                neighbors.push(rgba.get_pixel(x, y - 1));
            }
            if y < height - 1 {
                neighbors.push(rgba.get_pixel(x, y + 1));
            }

            if !neighbors.is_empty() {
                let variance = neighbors
                    .iter()
                    .map(|n| {
                        ((pixel[0] as i16 - n[0] as i16).abs() as f32
                            + (pixel[1] as i16 - n[1] as i16).abs() as f32
                            + (pixel[2] as i16 - n[2] as i16).abs() as f32)
                            / 3.0
                    })
                    .sum::<f32>()
                    / neighbors.len() as f32;

                complexity_sum += variance;
                count += 1;
            }
        }
    }

    if count == 0 {
        return 0.0;
    }

    // 归一化到 0-1
    let avg_complexity = complexity_sum / count as f32;
    (avg_complexity / 50.0).clamp(0.0, 1.0)
}

/// 检测图像是否已足够锐利
fn is_already_sharp_enough(image: &image::DynamicImage) -> bool {
    let complexity = estimate_image_complexity(image);
    // 复杂度高于 0.7 说明细节丰富，可能已足够锐利
    complexity > 0.7
}

/// 估算噪点水平（0.0-1.0）
/// 高噪点照片（高 ISO）锐化会放大噪点
fn estimate_noise_level(image: &image::DynamicImage) -> f32 {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    // 在平坦区域检测噪点
    let mut noise_scores = Vec::new();

    // 采样 10x10 区域
    let step = 20;
    for y in (0..height).step_by(step) {
        for x in (0..width).step_by(step) {
            // 检查 3x3 邻域
            let mut region_pixels = Vec::new();
            for dy in 0..3.min(height - y) {
                for dx in 0..3.min(width - x) {
                    region_pixels.push(rgba.get_pixel(x + dx, y + dy));
                }
            }

            if region_pixels.len() >= 4 {
                // 计算区域标准差
                let mean_r = region_pixels.iter().map(|p| p[0] as f32).sum::<f32>()
                    / region_pixels.len() as f32;
                let variance = region_pixels
                    .iter()
                    .map(|p| (p[0] as f32 - mean_r).powi(2))
                    .sum::<f32>()
                    / region_pixels.len() as f32;

                // 平坦区域的标准差代表噪点水平
                if variance < 100.0 {
                    // 只统计平坦区域
                    noise_scores.push(variance);
                }
            }
        }
    }

    if noise_scores.is_empty() {
        return 0.0;
    }

    let avg_noise = noise_scores.iter().sum::<f32>() / noise_scores.len() as f32;
    // 归一化：variance < 10 为低噪点，> 50 为高噪点
    (avg_noise / 50.0).min(1.0)
}

/// 智能自适应锐化（主入口）
///
/// 特性：
/// - 仅在需要时锐化（避免重复处理）
/// - 根据尺寸和内容自动选择参数
/// - 自动检测人像照片，降低肤色区域锐化
/// - 根据风景/人像选择不同的锐化策略
/// - 高性能（可选跳过）
fn smart_adaptive_sharpen(image: &image::DynamicImage, max_dim: u32) -> image::DynamicImage {
    // 智能决策：是否需要锐化
    if !should_apply_sharpening(image, max_dim) {
        return image.clone();
    }

    let (width, height) = image.dimensions();
    let larger_dim = width.max(height);

    // 检测肤色比例
    let skin_ratio = estimate_skin_ratio(image);
    let is_portrait = skin_ratio > 0.3;
    let complexity = estimate_image_complexity(image);

    // 根据尺寸、复杂度和内容类型选择参数
    let (radius, mut amount, threshold) = if is_portrait {
        // 人像照片：降低锐化强度
        if larger_dim < 2000 {
            (0.6, 0.3, 15) // 小图人像：非常轻微锐化
        } else if larger_dim < 4000 {
            (0.8, 0.4, 12) // 中图人像：轻微锐化
        } else {
            (1.0, 0.5, 10) // 大图人像：标准锐化但降低强度
        }
    } else {
        // 风景/其他照片：根据复杂度调整
        if larger_dim < 2000 {
            if complexity > 0.5 {
                (0.8, 0.6, 10) // 复杂小图：中度锐化
            } else {
                (0.6, 0.4, 15) // 简单小图：轻微锐化
            }
        } else if larger_dim < 4000 {
            if complexity > 0.5 {
                (1.0, 0.8, 6) // 复杂中图：强锐化
            } else {
                (0.8, 0.6, 8) // 简单中图：中度锐化
            }
        } else if complexity > 0.5 {
            (1.5, 1.0, 4) // 复杂超大图：最强锐化
        } else {
            (1.2, 0.8, 5) // 简单超大图：强锐化
        }
    };

    // 原图本身很小的情况：进一步降低锐化
    if larger_dim < 1500 {
        amount *= 0.7;
    }

    apply_usm_sharpen(image, radius, amount, threshold)
}

// ============================================================================
// USM 锐化核心函数
// ============================================================================

/// USM (Unsharp Mask) 锐化实现（智能版）
///
/// 特性：
/// - 自动检测并避开肤色区域
/// - 根据图像内容调整锐化强度
///
/// 原理：
/// 1. 对原图进行高斯模糊得到模糊图
/// 2. 原图 - 模糊图 = 高频细节（边缘）
/// 3. 原图 + amount * 高频细节 = 锐化图
///
/// 参数：
/// - radius: 高斯模糊半径，控制锐化范围
/// - amount: 锐化强度，0.5-1.5 常用
/// - threshold: 阈值，避免锐化平滑区域（减少噪点）
fn apply_usm_sharpen(
    image: &image::DynamicImage,
    radius: f32,
    amount: f32,
    threshold: u8,
) -> image::DynamicImage {
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();

    // 高斯模糊生成模糊图
    let sigma = radius;
    let blurred = gaussian_blur(&rgba, sigma);

    // 检测肤色比例
    let skin_ratio = estimate_skin_ratio(image);
    let is_portrait = skin_ratio > 0.3;

    // 创建输出图像
    let mut output = ImageBuffer::new(width, height);

    // USM 锐化公式：output = original + amount * (original - blurred)
    for y in 0..height {
        for x in 0..width {
            let orig_pixel = rgba.get_pixel(x, y);
            let blur_pixel = blurred.get_pixel(x, y);

            let mut out_pixel = Rgba([0u8; 4]);

            // 检测当前像素是否为肤色
            let skin_score = is_skin_color(orig_pixel[0], orig_pixel[1], orig_pixel[2]);
            let skin_factor = if is_portrait && skin_score > 0.3 {
                // 人像照片且是肤色区域：大幅降低锐化强度
                1.0 - skin_score * 0.8
            } else {
                // 非肤色区域或非人像：正常锐化
                1.0
            };

            for c in 0..3 {
                // 计算差异（高频细节）
                let diff = orig_pixel[c] as i16 - blur_pixel[c] as i16;

                // 阈值处理：小差异不锐化（避免噪点）
                if diff.abs() > threshold as i16 {
                    // 应用锐化强度和肤色因子
                    let effective_amount = amount * skin_factor;
                    let sharpened = orig_pixel[c] as i32 + (effective_amount * diff as f32) as i32;
                    out_pixel[c] = sharpened.clamp(0, 255) as u8;
                } else {
                    out_pixel[c] = orig_pixel[c];
                }
            }

            // Alpha 通道保持不变
            out_pixel[3] = orig_pixel[3];

            output.put_pixel(x, y, out_pixel);
        }
    }

    image::DynamicImage::ImageRgba8(output)
}

/// 高斯模糊实现（优化版：可分离卷积）
///
/// 性能优化：
/// - 使用可分离卷积：O(n²) → O(2n)
/// - 先水平卷积，再垂直卷积
/// - 性能提升 10 倍以上
pub(crate) fn gaussian_blur(
    image: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    sigma: f32,
) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let (width, height) = image.dimensions();

    // 高斯核大小（基于 sigma）
    let kernel_size = ((sigma * 6.0) as usize).max(3) | 1; // 确保奇数
    let half_kernel = kernel_size / 2;

    // 生成一维高斯核
    let kernel = generate_1d_gaussian_kernel(kernel_size, sigma);

    // Step 1: 水平卷积
    let mut temp = ImageBuffer::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let mut pixel = Rgba([0u8; 4]);
            for c in 0..4 {
                let mut sum = 0.0f32;
                let mut weight_sum = 0.0f32;
                for (kx, &weight) in kernel.iter().enumerate() {
                    let px = (x as i32 + kx as i32 - half_kernel as i32).clamp(0, width as i32 - 1)
                        as u32;
                    let sample_pixel = image.get_pixel(px, y);
                    sum += sample_pixel[c] as f32 * weight;
                    weight_sum += weight;
                }
                pixel[c] = (sum / weight_sum).clamp(0.0, 255.0) as u8;
            }
            temp.put_pixel(x, y, pixel);
        }
    }

    // Step 2: 垂直卷积
    let mut output = ImageBuffer::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let mut pixel = Rgba([0u8; 4]);
            for c in 0..4 {
                let mut sum = 0.0f32;
                let mut weight_sum = 0.0f32;
                for (ky, &weight) in kernel.iter().enumerate() {
                    let py = (y as i32 + ky as i32 - half_kernel as i32).clamp(0, height as i32 - 1)
                        as u32;
                    let sample_pixel = temp.get_pixel(x, py);
                    sum += sample_pixel[c] as f32 * weight;
                    weight_sum += weight;
                }
                pixel[c] = (sum / weight_sum).clamp(0.0, 255.0) as u8;
            }
            output.put_pixel(x, y, pixel);
        }
    }

    output
}

/// 生成一维高斯核
fn generate_1d_gaussian_kernel(size: usize, sigma: f32) -> Vec<f32> {
    let mut kernel = vec![0.0f32; size];
    let half = (size / 2) as f32;
    let two_sigma_sq = 2.0 * sigma * sigma;

    let mut sum = 0.0f32;
    for (x, val) in kernel.iter_mut().enumerate() {
        let dx = x as f32 - half;
        let value = (-dx * dx / two_sigma_sq).exp();
        *val = value;
        sum += value;
    }

    // 归一化
    for val in kernel.iter_mut() {
        *val /= sum;
    }

    kernel
}

// ============================================================================
// 智能色彩空间管理（保留供未来使用）
// ============================================================================

/// 智能色彩空间管理
///
/// 策略：
/// - 长边 ≤ 3000px：统一转 sRGB（网络分享标准）
/// - 长边 > 3000px：保持原色彩空间（专业用途）
/// - 检测 300dpi+ 高分辨率：保持原样（印刷级）
#[allow(dead_code)]
fn manage_color_space(
    image: image::DynamicImage,
    max_dim: u32,
    has_300dpi: bool,
) -> image::DynamicImage {
    // 高分辨率印刷级：完全保持原样
    if has_300dpi || max_dim > 4000 {
        return image;
    }

    // 网络分享级：统一转 sRGB
    if max_dim <= 3000 {
        // 简单转换：假设原图为 sRGB（大多数数码照片）
        // 注：完整实现需要 ICC Profile 管理
        return image;
    }

    // 中等尺寸：保持原样
    image
}

/// 检测图片是否为 300dpi 或更高分辨率
#[allow(dead_code)]
fn is_high_resolution_dpi(input_path: &Path) -> bool {
    // 尝试读取 EXIF 中的 DPI 信息
    if let Ok(file) = fs::File::open(input_path) {
        if let Ok(mmap) = unsafe { Mmap::map(&file) } {
            // 简单判断：如果文件很大且尺寸不大，可能是高 DPI
            if mmap.len() > 5 * 1024 * 1024 {
                return true;
            }
        }
    }
    false
}

pub fn path_self_healing(input_path: &Path) -> PathBuf {
    let path_str = input_path.to_string_lossy();

    if input_path.exists() && input_path.is_file() {
        return input_path.to_path_buf();
    }

    if let Some(file_name) = input_path.file_name().and_then(|n| n.to_str()) {
        if let Some(parent) = input_path.parent() {
            if parent.exists() {
                if let Ok(entries) = fs::read_dir(parent) {
                    for entry in entries.flatten() {
                        if let Some(name) = entry.file_name().to_str() {
                            if name.eq_ignore_ascii_case(file_name) {
                                let candidate = entry.path();
                                if candidate.is_file() {
                                    return candidate;
                                }
                            }
                        }
                    }
                }
            }

            if let Ok(entries) = fs::read_dir(parent) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.to_lowercase() == file_name.to_lowercase() {
                            let candidate = entry.path();
                            if candidate.is_file() {
                                return candidate;
                            }
                        }
                    }
                }
            }
        }
    }

    let normalized = path_str.replace("\\", "/");
    if normalized != path_str {
        let alt_path = Path::new(&normalized);
        if alt_path.exists() && alt_path.is_file() {
            return alt_path.to_path_buf();
        }
    }

    input_path.to_path_buf()
}

fn load_image_safe(input_path: &Path) -> Result<image::DynamicImage> {
    // 注意: 调用者(process_image)已做 path_self_healing,此处直接用输入路径,
    // 避免重复 stat 调用
    if let Ok(img) = load_image_mmap(input_path) {
        return Ok(img);
    }

    image::open(input_path).map_err(|e| anyhow::anyhow!("Failed to load image: {}", e))
}

fn load_image_mmap(input_path: &Path) -> Result<image::DynamicImage> {
    let file = fs::File::open(input_path)?;
    let file_size = file.metadata()?.len();

    if file_size > 200 * 1024 * 1024 {
        let mmap = unsafe { Mmap::map(&file)? };
        return image::load_from_memory(&mmap)
            .map_err(|e| anyhow::anyhow!("Failed to decode with mmap: {}", e));
    }

    drop(file);
    let bytes = fs::read(input_path)?;
    image::load_from_memory(&bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode from memory: {}", e))
}
