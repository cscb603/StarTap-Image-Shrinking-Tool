#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::egui;
use egui::IconData;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::fs;
use std::collections::VecDeque;
use rayon::prelude::*;
use anyhow::Result;
use crossbeam_channel::{Sender, Receiver, unbounded};

use img_parts::jpeg::Jpeg;
use bytes::Bytes;
use image::GenericImageView;
use fast_image_resize as fr;
use std::sync::atomic::{AtomicUsize, Ordering};

// 全局信号量，限制 sips 并发数，防止 DNG 批量处理时系统卡死
static SIPS_CONCURRENCY: AtomicUsize = AtomicUsize::new(0);
const MAX_SIPS_CONCURRENCY: usize = 4;

// --- Image Processing Logic ---

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
enum ProcessMode {
    WeChat,
    HD,
    Custom,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct AppConfig {
    mode: ProcessMode,
    custom_max_dim: u32,
    custom_quality: u8,
    custom_target_kb: u32,
    overwrite: bool,
    keep_original_name: bool,
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
        }
    }
}

#[derive(Clone, Debug)]
struct ProcessConfig {
    mode: ProcessMode,
    max_dim: u32,
    quality: u8,
    target_kb: u32,
    output_dir: Option<PathBuf>,
    overwrite: bool,
    keep_original_name: bool,
}

struct Processor {
    config: ProcessConfig,
}

impl Processor {
    fn new(config: ProcessConfig) -> Self {
        Self { config }
    }

    fn process_image(&self, input_path: &Path) -> Result<PathBuf> {
        let file_name_os = input_path.file_name().unwrap_or_default().to_string_lossy();
        if file_name_os.starts_with("._") {
            return Err(anyhow::anyhow!("跳过系统隐藏文件"));
        }

        let file_stem = input_path.file_stem().unwrap().to_string_lossy();
        let extension = input_path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        
        // 定义哪些格式被视为 RAW 格式，统一走 sips 流程
        let raw_extensions = ["dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw", "3fr"];
        let is_raw = raw_extensions.contains(&extension.as_str());

        let suffix = if self.config.mode == ProcessMode::WeChat { "_wechat" } else if self.config.mode == ProcessMode::HD { "_hd" } else { "_custom" };
        
        let output_dir = self.config.output_dir.clone().unwrap_or_else(|| input_path.parent().unwrap().to_path_buf());
        if !output_dir.exists() {
            fs::create_dir_all(&output_dir)?;
        }

        let output_path = if self.config.overwrite {
            input_path.to_path_buf()
        } else if self.config.keep_original_name {
            output_dir.join(format!("{}.jpg", file_stem))
        } else {
            output_dir.join(format!("{}{}.jpg", file_stem, suffix))
        };

        if is_raw {
            // 限制 sips 并发数
            while SIPS_CONCURRENCY.load(Ordering::SeqCst) >= MAX_SIPS_CONCURRENCY {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            SIPS_CONCURRENCY.fetch_add(1, Ordering::SeqCst);

            // 预处理路径：确保路径是绝对路径且规范化，处理外置硬盘问题
            let input_path_abs = fs::canonicalize(input_path).unwrap_or_else(|_| input_path.to_path_buf());
            let result = (|| -> Result<()> {
                let file_stem = input_path_abs.file_stem().and_then(|s| s.to_str()).unwrap_or("image");
                let file_name = input_path_abs.file_name().and_then(|s| s.to_str()).unwrap_or("image");

                // 1. 尝试使用 sips 提取内置预览图 (通常最快且最稳)
                // 很多 RAW 格式虽然 sips 不能直接转换，但能提取出预览图
                let mut preview_cmd = std::process::Command::new("sips");
                preview_cmd.arg("-e").arg("preview")
                    .arg(&input_path_abs)
                    .arg("--out").arg(&output_path);
                
                let _ = preview_cmd.output();
                if output_path.exists() {
                    // 检查提取出的预览图是否需要缩放
                    if let Ok(img) = image::open(&output_path) {
                        let (w, h) = img.dimensions();
                        if w > self.config.max_dim || h > self.config.max_dim {
                            let mut resize_cmd = std::process::Command::new("sips");
                            resize_cmd.arg("-Z").arg(self.config.max_dim.to_string());
                            resize_cmd.arg(&output_path);
                            let _ = resize_cmd.output();
                        }
                    }
                    return Ok(());
                }

                // 2. 尝试常规 sips 转换
                let mut cmd = std::process::Command::new("sips");
                cmd.arg("-s").arg("format").arg("jpeg");
                let quality = if self.config.target_kb > 0 && self.config.target_kb < 1000 { 75 } else { self.config.quality };
                cmd.arg("-s").arg("formatOptions").arg(quality.to_string());
                cmd.arg("-Z").arg(self.config.max_dim.to_string());
                cmd.arg(&input_path_abs).arg("--out").arg(&output_path);
                
                let mut child = cmd.spawn()?;
                let timeout = std::time::Duration::from_secs(30);
                let start = std::time::Instant::now();
                
                let status = loop {
                    match child.try_wait()? {
                        Some(status) => break status,
                        None => {
                            if start.elapsed() > timeout {
                                let _ = child.kill();
                                break std::process::ExitStatus::default(); // 故意返回失败
                            }
                            std::thread::sleep(std::time::Duration::from_millis(100));
                        }
                    }
                };

                if status.success() && output_path.exists() {
                    return Ok(());
                }

                // 3. 尝试 qlmanage (QuickLook 引擎)
                let temp_dir = std::env::temp_dir().join("rust_compressor_ql");
                let _ = fs::create_dir_all(&temp_dir);
                let mut ql_cmd = std::process::Command::new("qlmanage");
                ql_cmd.arg("-t")
                    .arg("-s").arg(self.config.max_dim.to_string())
                    .arg("-o").arg(&temp_dir)
                    .arg(&input_path_abs);
                
                // 增加超时控制
                if let Ok(mut child) = ql_cmd.spawn() {
                    let start = std::time::Instant::now();
                    loop {
                        if let Ok(Some(_)) = child.try_wait() { break; }
                        if start.elapsed().as_secs() > 30 {
                            let _ = child.kill();
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                }

                // 修复：qlmanage 生成的文件名可能是 file_stem.png 或 file_name.png
                let ql_file_1 = temp_dir.join(format!("{}.png", file_stem));
                let ql_file_2 = temp_dir.join(format!("{}.png", file_name));
                let ql_file = if ql_file_1.exists() { Some(ql_file_1) } 
                             else if ql_file_2.exists() { Some(ql_file_2) } 
                             else { None };

                if let Some(path) = ql_file {
                    let mut conv_cmd = std::process::Command::new("sips");
                    conv_cmd.arg("-s").arg("format").arg("jpeg")
                        .arg(&path)
                        .arg("--out").arg(&output_path);
                    let _ = conv_cmd.output();
                    let _ = fs::remove_file(path);
                    if output_path.exists() {
                        return Ok(());
                    }
                }

                // 4. 尝试最后的 AppleScript
                let script = format!(
                    "tell application \"Image Events\"\n\
                     try\n\
                     set theFile to (POSIX file \"{}\")\n\
                     set theImage to open theFile\n\
                     save theImage as JPEG in \"{}\"\n\
                     close theImage\n\
                     return \"OK\"\n\
                     on error err\n\
                     return err\n\
                     end try\n\
                     end tell",
                    input_path_abs.to_string_lossy(),
                    output_path.to_string_lossy()
                );
                
                let mut osascript_cmd = std::process::Command::new("osascript");
                osascript_cmd.arg("-e").arg(&script);
                
                // AppleScript 也要加超时，防止 TCC 权限弹窗导致死锁
                if let Ok(mut child) = osascript_cmd.spawn() {
                    let start = std::time::Instant::now();
                    let mut success = false;
                    loop {
                        if let Ok(Some(status)) = child.try_wait() { 
                            success = status.success();
                            break; 
                        }
                        if start.elapsed().as_secs() > 30 {
                            let _ = child.kill();
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(100));
                    }
                    if success && output_path.exists() {
                        return Ok(());
                    }
                }

                Err(anyhow::anyhow!("该机型 RAW 暂不支持"))
            })();

            SIPS_CONCURRENCY.fetch_sub(1, Ordering::SeqCst);
            result?;
        } else {
            // 对于普通 JPG/PNG/WebP，使用 Rust 高性能内核
            let img = image::open(input_path)?;
            let (width, height) = img.dimensions();
            
            // 计算缩放比例
            let scale = if width > self.config.max_dim || height > self.config.max_dim {
                let ratio_w = self.config.max_dim as f32 / width as f32;
                let ratio_h = self.config.max_dim as f32 / height as f32;
                ratio_w.min(ratio_h)
            } else {
                1.0
            };

            let new_width = (width as f32 * scale) as u32;
            let new_height = (height as f32 * scale) as u32;

            // 使用 fast_image_resize 4.2 API
            let src_image = fr::images::Image::from_vec_u8(
                width,
                height,
                img.to_rgba8().into_raw(),
                fr::PixelType::U8x4,
            )?;

            let mut dst_image = fr::images::Image::new(
                new_width,
                new_height,
                fr::PixelType::U8x4,
            );

            let mut resizer = fr::Resizer::new();
            resizer.resize(&src_image, &mut dst_image, None)?;

            // 编码为 JPEG
            let mut result_data = Vec::new();
            let encoder = jpeg_encoder::Encoder::new(&mut result_data, self.config.quality);
            encoder.encode(
                dst_image.buffer(),
                new_width as u16,
                new_height as u16,
                jpeg_encoder::ColorType::Rgba,
            )?;

            // 尝试保留 EXIF (如果是 JPG)
            if extension == "jpg" || extension == "jpeg" {
                if let Ok(input_bytes) = fs::read(input_path) {
                    if let Ok(input_jpeg) = Jpeg::from_bytes(Bytes::copy_from_slice(&input_bytes)) {
                        if let Some(exif_segment) = input_jpeg.segments().iter().find(|s| s.marker() == 0xE1) {
                            if let Ok(mut output_jpeg) = Jpeg::from_bytes(Bytes::copy_from_slice(&result_data)) {
                                output_jpeg.segments_mut().insert(1, exif_segment.clone());
                                result_data = output_jpeg.encoder().bytes().to_vec();
                            }
                        }
                    }
                }
            }

            fs::write(&output_path, result_data)?;
        }

        Ok(output_path)
    }
}

// --- App Logic ---

enum AppMessage {
    Started(usize), // total files
    Progress(usize, usize, String), // completed count, failed count, current file name
    Finished(usize, usize, Option<PathBuf>), // completed count, failed count, first output dir
    Error(String), // 错误消息
}

struct CompressorApp {
    // Persistent Config
    config: AppConfig,
    
    // UI State
    show_advanced: bool,
    custom_output_dir: Option<PathBuf>,
    
    // Warning state
    show_warning_step: u8, // 0: none, 1: first warning, 2: second warning
    pending_paths: Vec<PathBuf>,
    
    // Runtime
    is_processing: bool,
    total_files: usize,
    completed_files: usize,
    current_file_name: String,
    progress: f32, // 0.0 to 1.0
    status_text: String,
    
    // Communication
    rx: Receiver<AppMessage>,
    tx: Sender<AppMessage>,
}

impl CompressorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Custom fonts
        let mut fonts = egui::FontDefinitions::default();
        
        // Robust font loading - Prioritize system fonts
        let font_paths = if cfg!(target_os = "windows") {
            vec![
                "c:/windows/fonts/msyh.ttc",
                "c:/windows/fonts/msyhl.ttc",
                "c:/windows/fonts/msyh.ttf",
            ]
        } else if cfg!(target_os = "macos") {
            vec![
                "/System/Library/Fonts/PingFang.ttc",
                "/System/Library/Fonts/STHeiti Light.ttc",
                "/System/Library/Fonts/Hiragino Sans GB.ttc",
            ]
        } else {
            vec![]
        };

        for path in font_paths {
            if let Ok(data) = fs::read(path) {
                let font_data = egui::FontData::from_owned(data);
                // Hinting can make edges look "harder", let's see if default is okay
                fonts.font_data.insert(
                    "custom_font".to_owned(),
                    font_data,
                );
                fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "custom_font".to_owned());
                fonts.families.entry(egui::FontFamily::Monospace).or_default().push("custom_font".to_owned());
                break;
            }
        }
        
        cc.egui_ctx.set_fonts(fonts);
        
        // Visuals: UI UX Pro Max Professional SaaS Style
        let mut visuals = egui::Visuals::light();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(248, 250, 252); // Background
        visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59)); // Text
        visuals.widgets.noninteractive.rounding = 8.0.into();
        
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
        visuals.widgets.inactive.rounding = 8.0.into();
        
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(239, 246, 255);
        visuals.widgets.hovered.rounding = 8.0.into();
        
        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.rounding = 8.0.into();

        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235); // Primary
        visuals.window_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.window_rounding = 12.0.into();
        
        cc.egui_ctx.set_visuals(visuals);

        // Load Icon (Embedded PNG for reliability)
        // We will set this in main NativeOptions instead of here to avoid double initialization
        /*
        let icon_data = match image::load_from_memory(include_bytes!("../高速缩图图标.png")) {
            Ok(img) => {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                Some(IconData {
                    rgba: rgba.into_raw(),
                    width: w,
                    height: h,
                })
            },
            Err(e) => {
                eprintln!("Failed to load icon: {}", e);
                None
            },
        };
        
        if let Some(icon) = icon_data.clone() {
            cc.egui_ctx.send_viewport_cmd(egui::ViewportCommand::Icon(Some(std::sync::Arc::new(icon))));
        }
        */

        let (tx, rx) = unbounded();

        // Load config
        let config = if let Some(storage) = cc.storage {
            storage.get_string(eframe::APP_KEY)
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            AppConfig::default()
        };

        Self {
            config,
            show_advanced: false,
            custom_output_dir: None,
            show_warning_step: 0,
            pending_paths: Vec::new(),
            
            is_processing: false,
            total_files: 0,
            completed_files: 0,
            current_file_name: String::new(),
            progress: 0.0,
            status_text: "✨ 准备就绪，专业级主流 RAW/图片极速压缩".to_owned(),
            rx,
            tx,
        }
    }


    fn start_processing(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() { return; }
        
        if self.config.overwrite {
            self.pending_paths = paths;
            self.show_warning_step = 1;
        } else {
            self.execute_processing(paths);
        }
    }

    fn execute_processing(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() { return; }

        self.is_processing = true;
        self.progress = 0.0;
        self.status_text = "正在扫描文件...".to_owned();

        let tx = self.tx.clone();
        
        let config = match self.config.mode {
            ProcessMode::WeChat => ProcessConfig {
                mode: ProcessMode::WeChat,
                max_dim: 2048,
                quality: 85,
                target_kb: 900,
                output_dir: self.custom_output_dir.clone(),
                overwrite: self.config.overwrite,
                keep_original_name: self.config.keep_original_name,
            },
            ProcessMode::HD => ProcessConfig {
                mode: ProcessMode::HD,
                max_dim: 4096,
                quality: 95,
                target_kb: 5000,
                output_dir: self.custom_output_dir.clone(),
                overwrite: self.config.overwrite,
                keep_original_name: self.config.keep_original_name,
            },
            ProcessMode::Custom => ProcessConfig {
                mode: ProcessMode::Custom,
                max_dim: self.config.custom_max_dim,
                quality: self.config.custom_quality,
                target_kb: self.config.custom_target_kb,
                output_dir: self.custom_output_dir.clone(),
                overwrite: self.config.overwrite,
                keep_original_name: self.config.keep_original_name,
            }
        };
        
        std::thread::spawn(move || {
            let files = collect_files_recursive(paths);
            let total = files.len();
            
            if total == 0 {
                tx.send(AppMessage::Finished(0, 0, None)).unwrap();
                return;
            }

            tx.send(AppMessage::Started(total)).unwrap();
            
            let processor = Arc::new(Processor::new(config));
            let first_output_dir = Arc::new(std::sync::Mutex::new(None));
            let completed_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let failed_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));

            files.par_iter().for_each(|path| {
                let file_name = path.file_name().unwrap_or_default().to_string_lossy().to_string();
                match processor.process_image(path) {
                    Ok(out_path) => {
                        let mut first_dir = first_output_dir.lock().unwrap();
                        if first_dir.is_none() {
                            if let Some(parent) = out_path.parent() {
                                *first_dir = Some(parent.to_path_buf());
                            }
                        }
                        completed_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                    Err(e) => {
                        let _ = tx.send(AppMessage::Error(format!("文件 {} 处理失败: {}", file_name, e)));
                        failed_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                }
                
                let c = completed_count.load(std::sync::atomic::Ordering::SeqCst);
                let f = failed_count.load(std::sync::atomic::Ordering::SeqCst);
                tx.send(AppMessage::Progress(c, f, file_name)).unwrap();
            });

            let final_c = completed_count.load(std::sync::atomic::Ordering::SeqCst);
            let final_f = failed_count.load(std::sync::atomic::Ordering::SeqCst);
            let final_dir = first_output_dir.lock().unwrap().clone();
            tx.send(AppMessage::Finished(final_c, final_f, final_dir)).unwrap();
        });
    }
}

fn collect_files_recursive(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut all_files = Vec::new();
    let mut queue = VecDeque::from(paths);
    let raw_exts = ["dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw", "3fr"];
    let normal_exts = ["jpg", "jpeg", "png", "webp", "bmp"];

    while let Some(path) = queue.pop_front() {
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        // 彻底静默跳过 macOS 系统隐藏文件和以 ._ 开头的文件
        if file_name.starts_with('.') || file_name.starts_with("._") {
            continue;
        }

        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if normal_exts.contains(&ext_lower.as_str()) || raw_exts.contains(&ext_lower.as_str()) {
                    all_files.push(path);
                }
            }
        } else if path.is_dir() {
            if let Ok(entries) = fs::read_dir(&path) {
                let mut dir_entries: Vec<_> = entries.flatten().collect();
                // Sort entries for deterministic order
                dir_entries.sort_by_key(|e| e.path());
                for entry in dir_entries {
                    queue.push_back(entry.path());
                }
            }
        }
    }

    // 智能识别：如果在同一个文件夹下存在同名的 JPG 和 RAW，则跳过 RAW
    use std::collections::HashSet;
    let mut jpg_stems = HashSet::new();
    for path in &all_files {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        if ext == "jpg" || ext == "jpeg" {
            if let (Some(parent), Some(stem)) = (path.parent(), path.file_stem()) {
                jpg_stems.insert((parent.to_path_buf(), stem.to_os_string()));
            }
        }
    }

    let mut filtered_files = Vec::new();
    for path in all_files {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
        // 如果是 RAW，检查是否存在同名 JPG
        if raw_exts.contains(&ext.as_str()) {
            if let (Some(parent), Some(stem)) = (path.parent(), path.file_stem()) {
                if jpg_stems.contains(&(parent.to_path_buf(), stem.to_os_string())) {
                    continue; // 跳过同名 RAW
                }
            }
        }
        filtered_files.push(path);
    }

    // Final sort of all collected files
    filtered_files.sort();
    filtered_files
}

fn load_icon() -> Option<IconData> {
    match image::load_from_memory(include_bytes!("../高速缩图图标.png")) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            Some(IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            })
        },
        Err(_) => None,
    }
}

impl eframe::App for CompressorApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Ok(json) = serde_json::to_string(&self.config) {
            storage.set_string(eframe::APP_KEY, json);
        }
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Handle Messages
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMessage::Started(total) => {
                    self.total_files = total;
                    self.completed_files = 0;
                    self.status_text = format!("🚀 正在准备处理 {} 张图片...", total);
                    self.progress = 0.0;
                    self.current_file_name = String::new();
                }
                AppMessage::Progress(completed, failed, name) => {
                    self.completed_files = completed + failed;
                    self.current_file_name = name;
                    self.progress = self.completed_files as f32 / self.total_files as f32;
                    self.status_text = format!("正在处理: {} (成功: {}, 失败: {})", self.current_file_name, completed, failed);
                }
                AppMessage::Finished(completed, failed, first_dir) => {
                    self.is_processing = false;
                    self.progress = 1.0;
                    self.status_text = format!("✅ 处理完成！成功: {}, 失败: {}", completed, failed);
                    if let Some(path) = first_dir {
                        let _ = opener::open(path);
                    }
                }
                AppMessage::Error(err) => {
                    // 彻底取消弹窗，改为静默记录日志并更新状态
                    self.status_text = format!("⚠️ 警告: {}", err);
                }
            }
        }

        // Drag & Drop
        if !self.is_processing && !ctx.input(|i| i.raw.dropped_files.is_empty()) {
            let dropped_paths: Vec<PathBuf> = ctx.input(|i| {
                i.raw.dropped_files.iter().filter_map(|f| f.path.clone()).collect()
            });
            if !dropped_paths.is_empty() {
                self.start_processing(dropped_paths);
            }
        }

        // Header Panel (SaaS Style)
        egui::TopBottomPanel::top("header_panel")
            .frame(egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 15.0))
                .fill(egui::Color32::from_rgb(255, 255, 255))) // Clear white background
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 240.0) / 2.0);
                        ui.label(egui::RichText::new("📸").size(32.0));
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("星TAP 高清缩图")
                            .size(26.0)
                            .strong()
                            .color(egui::Color32::from_rgb(30, 41, 59))); // Slate 800
                    });
                    ui.add_space(5.0);
                    ui.label(egui::RichText::new("企业级图片处理内核 · 智能压缩 · 极速出片")
                        .size(12.0)
                        .color(egui::Color32::from_rgb(100, 116, 139))); // Slate 500
                });
            });

        // Bottom Status Panel
        egui::TopBottomPanel::bottom("status_panel")
            .frame(egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 15.0))
                .fill(egui::Color32::from_rgb(255, 255, 255)) // Consistency
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(241, 245, 249)))) // Subtle top border
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if self.is_processing {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&self.status_text).size(13.0).strong().color(egui::Color32::from_rgb(37, 99, 235)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{:.0}%", self.progress * 100.0)).size(13.0).strong().color(egui::Color32::from_rgb(30, 41, 59)));
                            });
                        });
                        ui.add_space(6.0);
                        let pb = egui::ProgressBar::new(self.progress)
                            .animate(true)
                            .rounding(4.0)
                            .fill(egui::Color32::from_rgb(37, 99, 235));
                        ui.add(pb);
                        ui.add_space(6.0);
                        if !self.current_file_name.is_empty() {
                            ui.add(egui::Label::new(
                                egui::RichText::new(format!("正在处理: {}", self.current_file_name))
                                    .size(10.0)
                                    .color(egui::Color32::from_rgb(100, 116, 139))
                            ).truncate(true));
                        }
                    } else {
                        ui.label(egui::RichText::new(&self.status_text).size(14.0).strong().color(egui::Color32::from_rgb(71, 85, 105)));
                    }
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new("星TAP 实验室 | 高性能 Rust 内核 v2026").size(10.0).color(egui::Color32::from_rgb(148, 163, 184)));
                });
            });

        // Central Content Panel
        egui::CentralPanel::default().frame(
            egui::Frame::none()
                .inner_margin(egui::Margin::symmetric(20.0, 10.0))
                .fill(egui::Color32::from_rgb(248, 250, 252)) // Slate 50 background
        ).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                // Settings Group
                ui.add_space(5.0);
                egui::Frame::none()
                    .fill(egui::Color32::WHITE)
                    .rounding(12.0)
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(226, 232, 240)))
                    .inner_margin(egui::Margin::same(15.0))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("✨ 版本更新 3.1").strong().color(egui::Color32::from_rgb(37, 99, 235)));
                                ui.add_space(10.0);
                                ui.label(egui::RichText::new("支持主流 RAW (CR2/CR3/DNG 等) | 极速原生并发 | 智能同名过滤").size(11.0).color(egui::Color32::from_rgb(100, 116, 139)));
                            });
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("选择输出模式").strong().size(15.0).color(egui::Color32::from_rgb(30, 41, 59)));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let arrow = if self.show_advanced { "🔼 收起参数" } else { "🔽 自定义参数" };
                                    if ui.button(egui::RichText::new(arrow).size(12.0).color(egui::Color32::from_rgb(37, 99, 235))).clicked() {
                                        self.show_advanced = !self.show_advanced;
                                    }
                                });
                            });
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                if ui.radio_value(&mut self.config.mode, ProcessMode::WeChat, "微信优化 (900KB)").clicked() {
                                    self.show_advanced = false;
                                }
                                ui.add_space(15.0);
                                if ui.radio_value(&mut self.config.mode, ProcessMode::HD, "高清无损 (5MB)").clicked() {
                                    self.show_advanced = false;
                                }
                                ui.add_space(15.0);
                                if ui.radio_value(&mut self.config.mode, ProcessMode::Custom, "自定义模式").clicked() {
                                    self.show_advanced = true;
                                }
                            });

                            ui.add_space(8.0);
                            let is_overwrite = self.config.overwrite;
                            ui.horizontal(|ui| {
                                if ui.checkbox(&mut self.config.overwrite, egui::RichText::new("覆盖原图 (不改名)").color(if is_overwrite { egui::Color32::RED } else { egui::Color32::from_rgb(71, 85, 105) })).changed() && self.config.overwrite {
                                    self.config.keep_original_name = false;
                                }
                                
                                ui.add_space(20.0);
                                
                                let can_keep_name = !is_overwrite;
                                let resp = ui.add_enabled(can_keep_name, egui::Checkbox::new(&mut self.config.keep_original_name, "保持原名 (导出到别处)"));
                                let resp = resp.on_disabled_hover_text("覆盖原图模式下默认保持原名");
                                if resp.changed() && self.config.keep_original_name {
                                    self.config.overwrite = false;
                                }
                            });
                        });
                    });

                if self.show_advanced {
                    ui.add_space(8.0);
                    egui::Frame::none()
                        .fill(egui::Color32::WHITE)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(226, 232, 240)))
                        .inner_margin(egui::Margin::same(15.0))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.vertical(|ui| {
                                egui::Grid::new("adv_grid").num_columns(2).spacing([25.0, 12.0]).show(ui, |ui| {
                                    ui.label(egui::RichText::new("长边限制 (px):").color(egui::Color32::from_rgb(71, 85, 105)));
                                    ui.add(egui::DragValue::new(&mut self.config.custom_max_dim).clamp_range(100..=10000).speed(10.0).suffix(" px"));
                                    ui.end_row();

                                    ui.label(egui::RichText::new("压缩质量 (1-100):").color(egui::Color32::from_rgb(71, 85, 105)));
                                    ui.add(egui::Slider::new(&mut self.config.custom_quality, 1..=100));
                                    ui.end_row();

                                    ui.label(egui::RichText::new("目标大小 (KB):").color(egui::Color32::from_rgb(71, 85, 105)));
                                    ui.horizontal(|ui| {
                                        ui.add(egui::DragValue::new(&mut self.config.custom_target_kb).clamp_range(0..=50000).speed(10.0).suffix(" KB"));
                                        ui.label(egui::RichText::new("(0 为不限制)").size(11.0).color(egui::Color32::GRAY));
                                    });
                                    ui.end_row();
                                });

                                ui.add_space(15.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("导出目录:").color(egui::Color32::from_rgb(71, 85, 105)));
                                    let display_path = self.custom_output_dir.as_ref()
                                        .map(|p| p.to_string_lossy().to_string())
                                        .unwrap_or_else(|| "默认 (原文件旁)".to_owned());
                                    
                                    ui.label(egui::RichText::new(display_path).size(12.0).color(egui::Color32::from_rgb(37, 99, 235)).strong());

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if self.custom_output_dir.is_some() && ui.button("重置").clicked() {
                                            self.custom_output_dir = None;
                                        }
                                        if ui.button("更改").clicked() {
                                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                                self.custom_output_dir = Some(path);
                                            }
                                        }
                                    });
                                });
                            });
                        });
                }

                ui.add_space(15.0);

                // Drop Zone (SaaS Style)
                let available_width = ui.available_width();
                let (rect, response) = ui.allocate_at_least(egui::vec2(available_width, 180.0), egui::Sense::click());
                
                let is_hovering = (ctx.input(|i| !i.raw.hovered_files.is_empty()) || response.hovered()) && !self.is_processing;
                
                let bg_color = if is_hovering { egui::Color32::from_rgb(239, 246, 255) } else { egui::Color32::WHITE };
                let stroke_color = if is_hovering { egui::Color32::from_rgb(37, 99, 235) } else { egui::Color32::from_rgb(226, 232, 240) };
                let stroke_width = if is_hovering { 2.5 } else { 1.5 };

                ui.painter().rect(rect, 16.0, bg_color, egui::Stroke::new(stroke_width, stroke_color));
                
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(40.0);
                        ui.label(egui::RichText::new("📥").size(40.0));
                        ui.add_space(8.0);
                        ui.label(egui::RichText::new("拖入图片或文件夹").size(16.0).strong().color(egui::Color32::from_rgb(30, 41, 59)));
                        ui.add_space(5.0);
                        ui.label(egui::RichText::new("支持 JPG, PNG, WEBP, DNG, RAW 等格式").size(12.0).color(egui::Color32::from_rgb(100, 116, 139)));
                        
                        ui.add_space(15.0);
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 2.0 - 55.0);
                            if ui.add(egui::Button::new(egui::RichText::new("浏览文件").color(egui::Color32::WHITE))
                                .fill(egui::Color32::from_rgb(37, 99, 235))
                                .rounding(6.0)
                            ).clicked() {
                                if let Some(paths) = rfd::FileDialog::new()
                                    .add_filter("图片文件", &["jpg", "jpeg", "png", "webp", "bmp", "dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw"])
                                    .pick_files() 
                                {
                                    self.start_processing(paths);
                                }
                            }
                        });
                    });
                });

                if response.clicked() && !self.is_processing {
                    if let Some(paths) = rfd::FileDialog::new().pick_files() {
                        self.start_processing(paths);
                    }
                }
            });
        });

        // Warning Dialogs
        if self.show_warning_step == 1 {
            egui::Window::new("⚠️ 危险操作确认 (1/2)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("您选择了「覆盖原图」模式").size(16.0).strong());
                        ui.add_space(10.0);
                        ui.label("这将永久替换您的原始图片文件，无法撤销。");
                        ui.label(egui::RichText::new("建议您先备份原始图片。").color(egui::Color32::RED));
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 4.0);
                            if ui.button("取消").clicked() {
                                self.show_warning_step = 0;
                                self.pending_paths.clear();
                            }
                            ui.add_space(20.0);
                            if ui.button(egui::RichText::new("我已知晓，继续").color(egui::Color32::RED)).clicked() {
                                self.show_warning_step = 2;
                            }
                        });
                        ui.add_space(10.0);
                    });
                });
        } else if self.show_warning_step == 2 {
            egui::Window::new("🛑 最后确认 (2/2)")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("确定要覆盖吗？").size(18.0).strong().color(egui::Color32::RED));
                        ui.add_space(10.0);
                        ui.label(format!("即将处理 {} 张图片并直接覆盖原文件。", self.pending_paths.len()));
                        ui.add_space(20.0);
                        ui.horizontal(|ui| {
                            ui.add_space(ui.available_width() / 4.0);
                            if ui.button("点错了，取消").clicked() {
                                self.show_warning_step = 0;
                                self.pending_paths.clear();
                            }
                            ui.add_space(20.0);
                            if ui.button(egui::RichText::new("确定覆盖").strong().color(egui::Color32::RED)).clicked() {
                                let paths = std::mem::take(&mut self.pending_paths);
                                self.execute_processing(paths);
                                self.show_warning_step = 0;
                            }
                        });
                        ui.add_space(10.0);
                    });
                });
        }
    }
}

fn main() -> eframe::Result<()> {
    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([540.0, 700.0])
        .with_title("星TAP 高清缩图")
        .with_resizable(false)
        .with_drag_and_drop(true);

    // Load icon safely
    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    
    eframe::run_native(
        "rust_image_compressor",
        options,
        Box::new(|cc| Box::new(CompressorApp::new(cc))),
    )
}
