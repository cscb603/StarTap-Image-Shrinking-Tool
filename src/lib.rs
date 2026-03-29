use anyhow::Result;
use bytes::Bytes;
use fast_image_resize as fr;
use image::GenericImageView;
use img_parts::jpeg::Jpeg;
use memmap2::Mmap;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

static SIPS_CONCURRENCY: AtomicUsize = AtomicUsize::new(0);
const MAX_SIPS_CONCURRENCY: usize = 4;

static THUMBNAIL_CACHE: Lazy<Mutex<HashMap<String, PathBuf>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            mode: ProcessMode::Custom,
            custom_max_dim: 3000,
            custom_quality: 95,
            custom_target_kb: 0,
            overwrite: false,
            keep_original_name: false,
            output_format: OutputFormat::Jpeg,
            color_space: ColorSpace::KeepOriginal,
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
}

pub struct Processor {
    config: ProcessConfig,
}

impl Processor {
    pub fn new(config: ProcessConfig) -> Self {
        Self { config }
    }

    pub fn process_image(&self, input_path: &Path) -> Result<PathBuf> {
        let healed_path = path_self_healing(input_path);
        let file_name_os = healed_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        if file_name_os.starts_with("._") {
            return Err(anyhow::anyhow!("跳过系统隐藏文件"));
        }

        let file_stem = healed_path.file_stem().unwrap().to_string_lossy();
        let extension = healed_path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        let raw_extensions = [
            "dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw", "3fr",
        ];
        let is_raw = raw_extensions.contains(&extension.as_str());

        let suffix = if self.config.mode == ProcessMode::WeChat {
            "_wechat"
        } else if self.config.mode == ProcessMode::HD {
            "_hd"
        } else {
            "_custom"
        };

        let output_dir = self
            .config
            .output_dir
            .clone()
            .unwrap_or_else(|| healed_path.parent().unwrap_or(Path::new(".")).to_path_buf());
        if !output_dir.exists() {
            fs::create_dir_all(&output_dir)?;
        }

        let output_ext = match self.config.output_format {
            OutputFormat::Jpeg => "jpg",
            OutputFormat::KeepOriginal => match extension.as_str() {
                "png" => "png",
                _ => "jpg",
            },
        };

        let output_path = if self.config.overwrite {
            healed_path.to_path_buf()
        } else if self.config.keep_original_name {
            output_dir.join(format!("{}.{}", file_stem, output_ext))
        } else {
            output_dir.join(format!("{}{}.{}", file_stem, suffix, output_ext))
        };

        if is_raw {
            self.process_raw(&healed_path, &output_path, &file_stem, &file_name_os)?;
        } else {
            self.process_normal(&healed_path, &output_path, &extension)?;
        }

        Ok(output_path)
    }

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
                if let Ok(img) = image::open(output_path) {
                    let (w, h) = img.dimensions();
                    if w > self.config.max_dim || h > self.config.max_dim {
                        let mut resize_cmd = std::process::Command::new("sips");
                        resize_cmd.arg("-Z").arg(self.config.max_dim.to_string());
                        resize_cmd.arg(output_path);
                        let _ = resize_cmd.output();
                    }
                }
                return Ok(());
            }

            let mut cmd = std::process::Command::new("sips");
            cmd.arg("-s").arg("format").arg("jpeg");
            let quality = self.config.quality;
            cmd.arg("-s").arg("formatOptions").arg(quality.to_string());
            cmd.arg("-Z").arg(self.config.max_dim.to_string());
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
            ql_cmd
                .arg("-t")
                .arg("-s")
                .arg(self.config.max_dim.to_string())
                .arg("-o")
                .arg(&temp_dir)
                .arg(&input_path_abs);

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

    fn process_normal(&self, input_path: &Path, output_path: &Path, extension: &str) -> Result<()> {
        let img = load_image_safe(input_path)?;
        let (width, height) = img.dimensions();

        let scale = if width > self.config.max_dim || height > self.config.max_dim {
            let ratio_w = self.config.max_dim as f32 / width as f32;
            let ratio_h = self.config.max_dim as f32 / height as f32;
            ratio_w.min(ratio_h)
        } else {
            1.0
        };

        let new_width = (width as f32 * scale) as u32;
        let new_height = (height as f32 * scale) as u32;

        let img_rgba = img.to_rgba8();

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
                let dynamic_img = image::DynamicImage::ImageRgba8(
                    image::ImageBuffer::from_raw(new_width, new_height, rgba_buf.to_vec())
                        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?,
                );
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
                let dynamic_img = image::DynamicImage::ImageRgba8(
                    image::ImageBuffer::from_raw(new_width, new_height, rgba_buf.to_vec())
                        .ok_or_else(|| anyhow::anyhow!("Failed to create image buffer"))?,
                );
                let mut cursor = Cursor::new(Vec::new());
                dynamic_img.write_to(&mut cursor, image::ImageFormat::WebP)?;
                result_data = cursor.into_inner();
            }
            _ => {
                let mut rgb_buf = Vec::with_capacity((new_width * new_height * 3) as usize);
                let mut has_alpha = false;

                for chunk in rgba_buf.chunks_exact(4) {
                    if chunk[3] < 255 {
                        has_alpha = true;
                        break;
                    }
                }

                if has_alpha {
                    for chunk in rgba_buf.chunks_exact(4) {
                        let a = chunk[3] as f32 / 255.0;
                        if a < 1.0 {
                            rgb_buf.push(((255.0 * (1.0 - a)) + (chunk[0] as f32 * a)) as u8);
                            rgb_buf.push(((255.0 * (1.0 - a)) + (chunk[1] as f32 * a)) as u8);
                            rgb_buf.push(((255.0 * (1.0 - a)) + (chunk[2] as f32 * a)) as u8);
                        } else {
                            rgb_buf.push(chunk[0]);
                            rgb_buf.push(chunk[1]);
                            rgb_buf.push(chunk[2]);
                        }
                    }
                } else {
                    for chunk in rgba_buf.chunks_exact(4) {
                        rgb_buf.push(chunk[0]);
                        rgb_buf.push(chunk[1]);
                        rgb_buf.push(chunk[2]);
                    }
                }

                let limit_bytes = if self.config.target_kb > 0 {
                    Some((self.config.target_kb as usize) * 1024)
                } else {
                    None
                };

                let encode_jpeg = |quality: u8| -> Result<Vec<u8>, anyhow::Error> {
                    let mut buf = Vec::new();
                    let encoder = jpeg_encoder::Encoder::new(&mut buf, quality);
                    encoder
                        .encode(
                            &rgb_buf,
                            new_width as u16,
                            new_height as u16,
                            jpeg_encoder::ColorType::Rgb,
                        )
                        .map_err(|e| anyhow::anyhow!("JPEG encoding failed: {}", e))?;
                    Ok(buf)
                };

                if let Some(limit) = limit_bytes {
                    let current_q = self.config.quality;
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
                        }
                    }
                } else {
                    result_data = encode_jpeg(self.config.quality)?;
                }

                if extension == "jpg" || extension == "jpeg" {
                    result_data = preserve_exif_safe(input_path, &result_data);
                }
            }
        }

        fs::write(output_path, result_data)?;
        Ok(())
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

pub fn app_config_to_process_config(
    config: &AppConfig,
    output_dir: Option<PathBuf>,
) -> ProcessConfig {
    match config.mode {
        ProcessMode::WeChat => ProcessConfig {
            mode: ProcessMode::WeChat,
            max_dim: 2048,
            quality: 95,
            target_kb: 900,
            output_dir,
            overwrite: config.overwrite,
            keep_original_name: config.keep_original_name,
            output_format: config.output_format,
            color_space: config.color_space,
        },
        ProcessMode::HD => ProcessConfig {
            mode: ProcessMode::HD,
            max_dim: 4096,
            quality: 95,
            target_kb: 5000,
            output_dir,
            overwrite: config.overwrite,
            keep_original_name: config.keep_original_name,
            output_format: config.output_format,
            color_space: config.color_space,
        },
        ProcessMode::Custom => ProcessConfig {
            mode: ProcessMode::Custom,
            max_dim: config.custom_max_dim,
            quality: config.custom_quality,
            target_kb: config.custom_target_kb,
            output_dir,
            overwrite: config.overwrite,
            keep_original_name: config.keep_original_name,
            output_format: config.output_format,
            color_space: config.color_space,
        },
    }
}

pub fn get_thumbnail_cache(path: &str) -> Option<PathBuf> {
    THUMBNAIL_CACHE
        .lock()
        .ok()
        .and_then(|cache| cache.get(path).cloned())
}

pub fn set_thumbnail_cache(path: &str, thumbnail_path: &Path) {
    if let Ok(mut cache) = THUMBNAIL_CACHE.lock() {
        cache.insert(path.to_string(), thumbnail_path.to_path_buf());
    }
}

pub fn clear_thumbnail_cache() {
    if let Ok(mut cache) = THUMBNAIL_CACHE.lock() {
        cache.clear();
    }
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
    let healed_path = path_self_healing(input_path);

    if let Ok(img) = load_image_mmap(&healed_path) {
        return Ok(img);
    }

    image::open(&healed_path).map_err(|e| anyhow::anyhow!("Failed to load image: {}", e))
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
