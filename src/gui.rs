//! GUI 层（人类交互界面）—— 4.1.0 版面骨架 + v4.2.0 三用途卡片
//!
//! 解耦约定：本文件只做 UI 与交互编排，压缩内核全部走 lib.rs
//! （app_config_to_process_config / Processor），与 CLI/AI-JSON 三方共用同一套语义。
//! UI 可独立更新，不碰内核；内核升级不破 UI。
//!
//! v4.2.0 变化（仅“选择输出模式”区块，其余 4.1.0 原版保留）：
//! - 三用途卡片：社交分享(平台预设) / 高清存档(不缩放最高画质) / 自定义(高级参数)
//! - 画质模式下拉：小而美(感知压缩) / 普通(标准压缩)
//! - 拖入即自动处理（4.1.0 已有）+ 优雅停止（当前图处理完才停，已输出图片保留）
//! - 处理中参数区自动折叠、拖放区收小
//! - 完成时持久化配置（用途/平台/画质记忆）

use anyhow::Result;
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use egui::IconData;
use num_cpus::get;
use rayon::prelude::*;
use std::collections::VecDeque;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use crate::cli::platform_preset;
use crate::runner::{collect_images, is_large_image, is_supported_image, load_config, save_config};
use rust_image_compressor::perceptual::{FocusMode, PerceptualOptions, QuantMode};
use rust_image_compressor::{
    app_config_to_process_config, AppConfig, ColorSpace, OutputFormat, ProcessMode, Processor,
};

#[allow(dead_code)]
#[derive(Debug, Clone)]
enum AppEvent {
    FilesAdded(Vec<PathBuf>),
    ProcessingStarted,
    ProcessingProgress(usize, usize), // file_id, success_flag
    ProcessingFinished(usize, usize),
    ClearFiles,
    ShowOutputFolder,
    ShowAbout,
    ToggleDarkMode,
}

#[derive(Debug, Clone, Default)]
struct FileItem {
    path: PathBuf,
    processed: bool,
    success: bool,
    #[allow(dead_code)]
    error: Option<String>,
}

#[allow(dead_code)]
struct ImageCompressorApp {
    dark_mode: bool,
    config: AppConfig,
    files: VecDeque<FileItem>,
    processing: bool,
    processed_count: usize,
    success_count: usize,
    show_about: bool,
    show_advanced: bool,
    custom_output_dir: Option<PathBuf>,
    about_version: String,
    /// 优雅停止旗标：置位后当前图片处理完即停，已输出图片保留
    stop_flag: Arc<AtomicBool>,
    /// 用户已点击“优雅停止”（UI 提示用）
    stop_requested: bool,
    /// 上一轮任务以“停止”收尾（状态栏提示用）
    stopped: bool,
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
}

impl ImageCompressorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (tx, rx) = unbounded();

        let config = load_config().unwrap_or_else(|_| AppConfig {
            custom_quality: 95,
            ..Default::default()
        });
        // 用途/平台/画质从配置记忆恢复（serde default 兜底：旧版配置文件也能正常加载）

        // 先设置视觉样式，避免窗口背景闪烁！
        let mut visuals = egui::Visuals::light();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.widgets.noninteractive.fg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59));
        visuals.widgets.noninteractive.corner_radius = 8.0.into();

        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
        visuals.widgets.inactive.corner_radius = 8.0.into();

        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(239, 246, 255);
        visuals.widgets.hovered.corner_radius = 8.0.into();

        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.corner_radius = 8.0.into();

        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        visuals.window_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.window_corner_radius = 12.0.into();
        visuals.panel_fill = egui::Color32::from_rgb(248, 250, 252);

        cc.egui_ctx.set_visuals(visuals);

        // 再设置字体
        let mut fonts = egui::FontDefinitions::default();

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
                fonts.font_data.insert(
                    "custom_font".to_owned(),
                    egui::FontData::from_owned(data).into(),
                );
                fonts
                    .families
                    .entry(egui::FontFamily::Proportional)
                    .or_default()
                    .insert(0, "custom_font".to_owned());
                fonts
                    .families
                    .entry(egui::FontFamily::Monospace)
                    .or_default()
                    .push("custom_font".to_owned());
                break;
            }
        }

        cc.egui_ctx.set_fonts(fonts);

        Self {
            dark_mode: false,
            config,
            files: VecDeque::new(),
            processing: false,
            processed_count: 0,
            success_count: 0,
            show_about: false,
            show_advanced: false,
            custom_output_dir: None,
            about_version: "v4.3.0".to_string(),
            stop_flag: Arc::new(AtomicBool::new(false)),
            stop_requested: false,
            stopped: false,
            tx,
            rx,
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        // 如果列表不为空，说明是新一轮任务，清空并重置计数
        if !self.files.is_empty() {
            self.clear_files();
        }

        for path in paths {
            if path.is_dir() {
                // GUI 总是递归收集（匹配 UI 上"自动递归处理子目录"的文案）
                let mut temp = Vec::new();
                collect_images(&path, &mut temp, true, 0, &mut Vec::new());
                for p in temp {
                    self.files.push_back(FileItem {
                        path: p,
                        processed: false,
                        success: false,
                        error: None,
                    });
                }
            } else if path.is_file() && is_supported_image(&path) {
                self.files.push_back(FileItem {
                    path,
                    processed: false,
                    success: false,
                    error: None,
                });
            }
        }
    }

    fn clear_files(&mut self) {
        self.files.clear();
        self.processed_count = 0;
        self.success_count = 0;
        self.stopped = false;
    }

    /// 三用途统一入口：social(平台预设) / archive(不缩放最高画质) / custom(高级参数)
    /// 全部经 lib.rs app_config_to_process_config —— 与 CLI/AI-JSON 完全同一套语义。
    fn start_processing(&mut self) {
        if self.files.is_empty() || self.processing {
            return;
        }

        self.processing = true;
        self.processed_count = 0;
        self.success_count = 0;
        self.stopped = false;
        self.stop_requested = false;
        self.stop_flag.store(false, Ordering::SeqCst);

        let files: Vec<FileItem> = self.files.clone().into_iter().collect();
        let mut config = self.config.clone();
        let custom_output_dir = self.custom_output_dir.clone();
        let tx = self.tx.clone();
        let stop_flag = self.stop_flag.clone();

        // 用途驱动参数（EXIF 保留 / 智能锐化 / 色彩空间 等内核细节全部在 lib.rs，不受影响）
        match config.usage_mode.as_str() {
            "social" => {
                // 平台预设：长边/质量/体积线 + 是否强制 sRGB（防平台二压）
                if let Some((max_dim, quality, target_kb, srgb)) = platform_preset(&config.platform)
                {
                    config.custom_max_dim = max_dim;
                    config.custom_quality = quality;
                    config.custom_target_kb = target_kb;
                    if srgb {
                        config.color_space = ColorSpace::ConvertToSRGB;
                    }
                }
            }
            "archive" => {
                // lib.rs archive 分支：不缩放(0) + Q100 + 不限体积；此处无需覆盖
            }
            _ => {
                // 自定义：走高级面板的 custom_* 参数
                config.mode = ProcessMode::Custom;
            }
        }

        // 画质模式：小而美(感知压缩) / 普通。高清存档已是最高画质，感知管线不介入。
        let perceptual_on = config.quality_mode == "perceptual" && config.usage_mode != "archive";
        let platform_for_perceptual = config.platform.clone();

        let _ = std::thread::spawn(move || {
            let mut processor_config = app_config_to_process_config(&config, custom_output_dir);
            processor_config.perceptual = if perceptual_on {
                Some(PerceptualOptions {
                    denoise_strength: 25,
                    focus_mode: FocusMode::Auto,
                    quant_mode: QuantMode::Csf,
                    quality_ceil: 100,
                    budget_kb: None,
                    platform: Some(platform_for_perceptual),
                })
            } else {
                None
            };
            let processor = Processor::new(processor_config);
            let total = files.len();
            let success_count = AtomicUsize::new(0);

            // 分桶调度：大图（TIFF / 超大文件 / 超高像素）串行，小图并行。
            // 目的：多张超大 TIFF 若并行解码会瞬间吃满内存触发 OOM；
            //       串行把内存峰值锁死在「单张」，处理完释放再下一张，绝不叠加。
            let mut big: Vec<(usize, PathBuf)> = Vec::new();
            let mut small: Vec<(usize, PathBuf)> = Vec::new();
            for (index, item) in files.iter().enumerate() {
                if is_large_image(&item.path) {
                    big.push((index, item.path.clone()));
                } else {
                    small.push((index, item.path.clone()));
                }
            }

            // 小图并行（rayon 全局池，GUI 已封顶 4 线程，内存可控）
            // 优雅停止：已置位则跳过未开始的图；正在处理的图完整跑完（原子写不破坏输出）
            small.par_iter().for_each(|(index, path)| {
                if stop_flag.load(Ordering::SeqCst) {
                    return;
                }
                let is_success = processor.process_image(path).is_ok();
                if is_success {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
                let _ = tx.send(AppEvent::ProcessingProgress(
                    *index,
                    if is_success { 1 } else { 0 },
                ));
            });

            // 大图串行（一次一张，内存峰值 = 单张，绝不叠加）
            for (index, path) in &big {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                let is_success = processor.process_image(path).is_ok();
                if is_success {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }
                let _ = tx.send(AppEvent::ProcessingProgress(
                    *index,
                    if is_success { 1 } else { 0 },
                ));
            }

            let _ = tx.send(AppEvent::ProcessingFinished(
                total,
                success_count.load(Ordering::Relaxed),
            ));
        });
    }

    /// 三用途卡片（社交分享 / 高清存档 / 自定义）：文字上下左右居中，选中蓝边高亮
    fn usage_card(
        ui: &mut egui::Ui,
        selected: bool,
        width: f32,
        title: &str,
        subtitle: &str,
    ) -> egui::Response {
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, 58.0), egui::Sense::click());
        let (fill, stroke, title_color) = if selected {
            (
                egui::Color32::from_rgb(239, 246, 255),
                egui::Stroke::new(1.5, egui::Color32::from_rgb(37, 99, 235)),
                egui::Color32::from_rgb(37, 99, 235),
            )
        } else if response.hovered() {
            (
                egui::Color32::from_rgb(248, 250, 252),
                egui::Stroke::new(1.0, egui::Color32::from_rgb(148, 163, 184)),
                egui::Color32::from_rgb(30, 41, 59),
            )
        } else {
            (
                egui::Color32::WHITE,
                egui::Stroke::new(1.0, egui::Color32::from_rgb(226, 232, 240)),
                egui::Color32::from_rgb(30, 41, 59),
            )
        };
        ui.painter().rect(
            rect,
            egui::CornerRadius::same(10),
            fill,
            stroke,
            egui::StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center() - egui::vec2(0.0, 10.0),
            egui::Align2::CENTER_CENTER,
            title,
            egui::FontId::proportional(14.0),
            title_color,
        );
        ui.painter().text(
            rect.center() + egui::vec2(0.0, 12.0),
            egui::Align2::CENTER_CENTER,
            subtitle,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(100, 116, 139),
        );
        response
    }
}

impl eframe::App for ImageCompressorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        while let Ok(event) = self.rx.try_recv() {
            match event {
                AppEvent::FilesAdded(paths) => self.add_files(paths),
                AppEvent::ProcessingStarted => self.processing = true,
                AppEvent::ProcessingProgress(index, success_flag) => {
                    if let Some(item) = self.files.get_mut(index) {
                        item.processed = true;
                        item.success = success_flag > 0;
                        if success_flag > 0 {
                            self.success_count += 1;
                        }
                    }
                    self.processed_count += 1;
                }
                AppEvent::ProcessingFinished(total, success) => {
                    self.processing = false;
                    if self.stop_requested {
                        // 优雅停止收尾：processed_count 保持实际完成数，标记已停止
                        self.stopped = true;
                    } else {
                        self.processed_count = total;
                    }
                    self.success_count = success;
                    self.stop_requested = false;
                    // 配置记忆：用途/平台/画质/覆盖等随任务完成持久化
                    let _ = save_config(&self.config);
                    // 不清空列表，保留显示成功数量，等待用户查看
                }
                AppEvent::ClearFiles => self.clear_files(),
                AppEvent::ShowOutputFolder => {
                    if let Some(first) = self.files.front() {
                        if let Some(dir) = first.path.parent() {
                            let _ = opener::open(dir);
                        }
                    }
                }
                AppEvent::ShowAbout => self.show_about = true,
                AppEvent::ToggleDarkMode => {}
            }
        }

        // 处理中保持界面刷新（进度条/百分比实时走动）
        if self.processing {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }

        if !self.processing {
            let files_dropped = ctx.input(|i| i.raw.dropped_files.clone());
            if !files_dropped.is_empty() {
                let paths: Vec<PathBuf> =
                    files_dropped.into_iter().filter_map(|f| f.path).collect();
                self.add_files(paths);
                self.start_processing();
            }
        }

        egui::TopBottomPanel::top("header_panel")
            .exact_height(120.0)
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(20, 15))
                    .fill(egui::Color32::from_rgb(255, 255, 255)),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.add_space((ui.available_width() - 240.0) / 2.0);
                        ui.label(egui::RichText::new("📸").size(32.0));
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("图片高速压缩")
                                .size(26.0)
                                .strong()
                                .color(egui::Color32::from_rgb(30, 41, 59)),
                        );
                    });
                    ui.add_space(5.0);
                    ui.label(
                        egui::RichText::new("高性能 Rust 处理内核 · 极速压缩")
                            .size(12.0)
                            .color(egui::Color32::from_rgb(100, 116, 139)),
                    );
                });
            });

        egui::TopBottomPanel::bottom("status_panel")
            .exact_height(90.0)
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(20, 15))
                    .fill(egui::Color32::from_rgb(255, 255, 255))
                    .stroke(egui::Stroke::new(
                        1.0,
                        egui::Color32::from_rgb(241, 245, 249),
                    )),
            )
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if self.processing {
                        ui.horizontal(|ui| {
                            let status_text = if self.stop_requested {
                                "正在优雅停止…当前图片处理完即停".to_string()
                            } else {
                                format!("正在处理 {} 个文件...", self.processed_count)
                            };
                            ui.label(
                                egui::RichText::new(status_text)
                                    .size(13.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(37, 99, 235)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{:.0}%",
                                            (self.processed_count as f32 / self.files.len() as f32
                                                * 100.0)
                                        ))
                                        .size(13.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(30, 41, 59)),
                                    );
                                },
                            );
                        });
                        ui.add_space(6.0);
                        let pb = egui::ProgressBar::new(
                            self.processed_count as f32 / self.files.len() as f32,
                        )
                        .animate(true)
                        .corner_radius(4.0)
                        .fill(egui::Color32::from_rgb(37, 99, 235));
                        ui.add(pb);
                    } else {
                        // 处理完成，显示结果
                        if self.stopped {
                            ui.label(
                                egui::RichText::new(format!(
                                    "⏸ 已停止 | 完成 {} 个（已输出图片全部保留）",
                                    self.success_count
                                ))
                                .size(14.0)
                                .strong()
                                .color(egui::Color32::from_rgb(71, 85, 105)),
                            );
                        } else if self.processed_count > 0 {
                            ui.label(
                                egui::RichText::new(format!(
                                    "✨ 处理完成 | 成功 {} 个 | 共 {} 个",
                                    self.success_count, self.processed_count
                                ))
                                .size(14.0)
                                .strong()
                                .color(egui::Color32::from_rgb(71, 85, 105)),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new(format!(
                                    "✨ 准备就绪，待处理 {} 个文件",
                                    self.files.len()
                                ))
                                .size(14.0)
                                .strong()
                                .color(egui::Color32::from_rgb(71, 85, 105)),
                            );
                        }
                    }
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("星TAP 实验室 | 高性能 Rust 内核 v4.2 · 感知压缩引擎")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                    ui.label(
                        egui::RichText::new(
                            "© 2026 星TAP实验室 · EXIF 保留 · 大图防爆 · 平台防二压",
                        )
                        .size(9.0)
                        .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                });
            });

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(20, 10))
                    .fill(egui::Color32::from_rgb(248, 250, 252)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(5.0);

                    if self.processing {
                        // ===== 处理中：参数区自动折叠为一行摘要 =====
                        egui::Frame::new()
                            .fill(egui::Color32::WHITE)
                            .corner_radius(12.0)
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(226, 232, 240),
                            ))
                            .inner_margin(egui::Margin::same(12))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.horizontal(|ui| {
                                    let usage_label = match self.config.usage_mode.as_str() {
                                        "social" => "社交分享",
                                        "archive" => "高清存档",
                                        _ => "自定义",
                                    };
                                    let plat_label = match self.config.platform.as_str() {
                                        "wechat" => "微信 (全版本)",
                                        "wechat-new" => "微信-new (iOS 新)",
                                        "xiaohongshu" => "小红书",
                                        "instagram" => "Instagram",
                                        "general" => "通用 (中画幅)",
                                        _ => "微信 (全版本)",
                                    };
                                    let quality_label =
                                        if self.config.quality_mode == "perceptual" {
                                            "小而美"
                                        } else {
                                            "普通"
                                        };
                                    let summary = if self.config.usage_mode == "social" {
                                        format!(
                                            "▶ {} · {} · {}",
                                            usage_label, plat_label, quality_label
                                        )
                                    } else {
                                        format!("▶ {} · {}", usage_label, quality_label)
                                    };
                                    ui.label(
                                        egui::RichText::new(summary)
                                            .size(13.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(37, 99, 235)),
                                    );
                                });
                            });
                    } else {
                        // ===== 空闲：完整参数卡片（4.1.0 骨架 + 三用途卡片） =====
                        egui::Frame::new()
                            .fill(egui::Color32::WHITE)
                            .corner_radius(12.0)
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(226, 232, 240),
                            ))
                            .inner_margin(egui::Margin::same(15))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("✨ 内核升级 v4.2")
                                                .strong()
                                                .color(egui::Color32::from_rgb(37, 99, 235)),
                                        );
                                        ui.add_space(10.0);
                                        ui.label(
                                            egui::RichText::new(
                                                "感知压缩 | EXIF 保留 | 大图防爆 | 平台防二压",
                                            )
                                            .size(11.0)
                                            .color(egui::Color32::from_rgb(100, 116, 139)),
                                        );
                                    });
                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("选择用途")
                                                .strong()
                                                .size(15.0)
                                                .color(egui::Color32::from_rgb(30, 41, 59)),
                                        );
                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                let arrow = if self.show_advanced {
                                                    "收起参数"
                                                } else {
                                                    "自定义参数"
                                                };
                                                if ui
                                                    .button(
                                                        egui::RichText::new(arrow).size(12.0).color(
                                                            egui::Color32::from_rgb(37, 99, 235),
                                                        ),
                                                    )
                                                    .clicked()
                                                {
                                                    self.show_advanced = !self.show_advanced;
                                                }
                                            },
                                        );
                                    });
                                    ui.add_space(10.0);

                                    // ===== 三用途卡片（等宽、留白统一、文字上下左右居中） =====
                                    let gap = 8.0;
                                    let card_w = (ui.available_width() - 2.0 * gap) / 3.0;
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = gap;
                                        if Self::usage_card(
                                            ui,
                                            self.config.usage_mode == "social",
                                            card_w,
                                            "📤 社交分享",
                                            "按平台压到最优体积",
                                        )
                                        .clicked()
                                        {
                                            self.config.usage_mode = "social".to_string();
                                        }
                                        if Self::usage_card(
                                            ui,
                                            self.config.usage_mode == "archive",
                                            card_w,
                                            "💎 高清存档",
                                            "不缩放 · 最高画质",
                                        )
                                        .clicked()
                                        {
                                            self.config.usage_mode = "archive".to_string();
                                        }
                                        if Self::usage_card(
                                            ui,
                                            self.config.usage_mode == "custom",
                                            card_w,
                                            "⚙️ 自定义",
                                            "高级参数自由调",
                                        )
                                        .clicked()
                                        {
                                            self.config.usage_mode = "custom".to_string();
                                            self.show_advanced = true;
                                        }
                                    });

                                    ui.add_space(10.0);

                                    // ===== 用途附属选项（二级菜单，左缩进对齐） =====
                                    match self.config.usage_mode.as_str() {
                                        "social" => {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("发到哪:")
                                                        .strong()
                                                        .size(13.0)
                                                        .color(egui::Color32::from_rgb(30, 41, 59)),
                                                );
                                                let plat_label =
                                                    match self.config.platform.as_str() {
                                                        "wechat" => "微信 (全版本)",
                                                        "wechat-new" => "微信-new (iOS 新)",
                                                        "xiaohongshu" => "小红书",
                                                        "instagram" => "Instagram",
                                                        "general" => "通用 (中画幅)",
                                                        _ => "微信 (全版本)",
                                                    };
                                                egui::ComboBox::from_id_salt("platform_combo")
                                                    .selected_text(
                                                        egui::RichText::new(plat_label).color(
                                                            egui::Color32::from_rgb(37, 99, 235),
                                                        ),
                                                    )
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.config.platform,
                                                            "wechat".to_string(),
                                                            "微信 (全版本)",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.config.platform,
                                                            "wechat-new".to_string(),
                                                            "微信-new (iOS 新)",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.config.platform,
                                                            "xiaohongshu".to_string(),
                                                            "小红书",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.config.platform,
                                                            "instagram".to_string(),
                                                            "Instagram",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.config.platform,
                                                            "general".to_string(),
                                                            "通用 (中画幅)",
                                                        );
                                                    });
                                                ui.add_space(12.0);
                                                ui.label(
                                                    egui::RichText::new("画质:")
                                                        .strong()
                                                        .size(13.0)
                                                        .color(egui::Color32::from_rgb(30, 41, 59)),
                                                );
                                                let qm_label =
                                                    if self.config.quality_mode == "perceptual" {
                                                        "小而美 (推荐)"
                                                    } else {
                                                        "普通"
                                                    };
                                                egui::ComboBox::from_id_salt("quality_combo")
                                                    .selected_text(
                                                        egui::RichText::new(qm_label).color(
                                                            egui::Color32::from_rgb(37, 99, 235),
                                                        ),
                                                    )
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.config.quality_mode,
                                                            "perceptual".to_string(),
                                                            "小而美 (推荐)",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.config.quality_mode,
                                                            "normal".to_string(),
                                                            "普通",
                                                        );
                                                    });
                                            });
                                            ui.add_space(4.0);
                                            ui.label(
                                                egui::RichText::new(
                                                    "自动卡住平台体积线，防止二次压缩糊图；小而美 = 同体积画质更好",
                                                )
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                        }
                                        "archive" => {
                                            ui.label(
                                                egui::RichText::new(
                                                    "保持原尺寸 · 最高画质 Q100 · 不限体积 · 保留原色域与 EXIF",
                                                )
                                                .size(11.0)
                                                .color(egui::Color32::from_rgb(148, 163, 184)),
                                            );
                                        }
                                        _ => {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new("画质:")
                                                        .strong()
                                                        .size(13.0)
                                                        .color(egui::Color32::from_rgb(30, 41, 59)),
                                                );
                                                let qm_label =
                                                    if self.config.quality_mode == "perceptual" {
                                                        "小而美 (推荐)"
                                                    } else {
                                                        "普通"
                                                    };
                                                egui::ComboBox::from_id_salt("quality_combo_c")
                                                    .selected_text(
                                                        egui::RichText::new(qm_label).color(
                                                            egui::Color32::from_rgb(37, 99, 235),
                                                        ),
                                                    )
                                                    .show_ui(ui, |ui| {
                                                        ui.selectable_value(
                                                            &mut self.config.quality_mode,
                                                            "perceptual".to_string(),
                                                            "小而美 (推荐)",
                                                        );
                                                        ui.selectable_value(
                                                            &mut self.config.quality_mode,
                                                            "normal".to_string(),
                                                            "普通",
                                                        );
                                                    });
                                                ui.add_space(10.0);
                                                ui.label(
                                                    egui::RichText::new(
                                                        "长边/质量/目标大小在「自定义参数」面板设置",
                                                    )
                                                    .size(11.0)
                                                    .color(egui::Color32::from_rgb(148, 163, 184)),
                                                );
                                            });
                                        }
                                    }

                                    ui.add_space(8.0);
                                    let is_overwrite = self.config.overwrite;
                                    ui.horizontal(|ui| {
                                        ui.checkbox(
                                            &mut self.config.overwrite,
                                            egui::RichText::new("覆盖原图 (不改名)").color(
                                                if is_overwrite {
                                                    egui::Color32::RED
                                                } else {
                                                    egui::Color32::from_rgb(71, 85, 105)
                                                },
                                            ),
                                        );
                                        ui.add_space(20.0);
                                        let can_keep_name = !is_overwrite;
                                        ui.add_enabled(
                                            can_keep_name,
                                            egui::Checkbox::new(
                                                &mut self.config.keep_original_name,
                                                "保持原名 (导出到别处)",
                                            ),
                                        );
                                    });
                                });
                            });
                    }

                    if self.show_advanced && !self.processing {
                        ui.add_space(8.0);
                        egui::Frame::new()
                            .fill(egui::Color32::WHITE)
                            .corner_radius(12.0)
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(226, 232, 240),
                            ))
                            .inner_margin(egui::Margin::same(15))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.vertical(|ui| {
                                    egui::Grid::new("adv_grid")
                                        .num_columns(2)
                                        .spacing([25.0, 12.0])
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new("长边限制 (px):")
                                                    .color(egui::Color32::from_rgb(71, 85, 105)),
                                            );
                                            ui.add(
                                                egui::DragValue::new(
                                                    &mut self.config.custom_max_dim,
                                                )
                                                .range(100..=10000)
                                                .speed(10.0)
                                                .suffix(" px"),
                                            )
                                            .on_hover_text(
                                                "限制图片最长边像素，超出自动等比缩小（100–10000）",
                                            );
                                            ui.end_row();

                                            ui.label(
                                                egui::RichText::new("压缩质量 (1-100):")
                                                    .color(egui::Color32::from_rgb(71, 85, 105)),
                                            );
                                            ui.add(egui::Slider::new(
                                                &mut self.config.custom_quality,
                                                1..=100,
                                            ))
                                            .on_hover_text(
                                                "数值越高越清晰、体积越大（仅自定义用途生效）",
                                            );
                                            ui.end_row();

                                            ui.label(
                                                egui::RichText::new("目标大小 (KB):")
                                                    .color(egui::Color32::from_rgb(71, 85, 105)),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    egui::DragValue::new(
                                                        &mut self.config.custom_target_kb,
                                                    )
                                                    .range(0..=50000)
                                                    .speed(10.0)
                                                    .suffix(" KB"),
                                                )
                                                .on_hover_text("接近原图体积时处理更慢");
                                                ui.label(
                                                    egui::RichText::new("(0 为不限制)")
                                                        .size(11.0)
                                                        .color(egui::Color32::GRAY),
                                                );
                                            });
                                            ui.end_row();

                                            ui.label(
                                                egui::RichText::new("色彩子采样:")
                                                    .color(egui::Color32::from_rgb(71, 85, 105)),
                                            )
                                            .on_hover_text(
                                                "照片默认 4:2:0（更省体积）；截图/文字用 4:4:4 防模糊",
                                            );
                                            egui::ComboBox::from_label("")
                                                .selected_text(match self.config.subsampling.as_str() {
                                                    "444" => "截图文字 4:4:4",
                                                    "422" => "平衡 4:2:2",
                                                    _ => "照片 4:2:0 (默认)",
                                                })
                                                .show_ui(ui, |ui| {
                                                    ui.selectable_value(
                                                        &mut self.config.subsampling,
                                                        "420".to_string(),
                                                        "照片 4:2:0 (默认)",
                                                    );
                                                    ui.selectable_value(
                                                        &mut self.config.subsampling,
                                                        "444".to_string(),
                                                        "截图文字 4:4:4",
                                                    );
                                                    ui.selectable_value(
                                                        &mut self.config.subsampling,
                                                        "422".to_string(),
                                                        "平衡 4:2:2",
                                                    );
                                                });
                                            ui.end_row();
                                        });

                                    ui.add_space(15.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("导出目录:")
                                                .color(egui::Color32::from_rgb(71, 85, 105)),
                                        )
                                        .on_hover_text("留空 = 与原图同目录");
                                        let display_path = self
                                            .custom_output_dir
                                            .as_ref()
                                            .map(|p| p.to_string_lossy().to_string())
                                            .unwrap_or_else(|| "默认 (原文件旁)".to_owned());

                                        ui.label(
                                            egui::RichText::new(display_path)
                                                .size(12.0)
                                                .color(egui::Color32::from_rgb(37, 99, 235))
                                                .strong(),
                                        );

                                        ui.with_layout(
                                            egui::Layout::right_to_left(egui::Align::Center),
                                            |ui| {
                                                if self.custom_output_dir.is_some()
                                                    && ui.button("重置").clicked()
                                                {
                                                    self.custom_output_dir = None;
                                                }
                                                if ui.button("更改").clicked() {
                                                    if let Some(path) =
                                                        rfd::FileDialog::new().pick_folder()
                                                    {
                                                        self.custom_output_dir = Some(path);
                                                    }
                                                }
                                            },
                                        );
                                    });

                                    ui.add_space(10.0);
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("导出格式:")
                                                .color(egui::Color32::from_rgb(71, 85, 105)),
                                        )
                                        .on_hover_text("JPG 体积最小；保持原始仅对 PNG 生效");
                                        ui.radio_value(
                                            &mut self.config.output_format,
                                            OutputFormat::Jpeg,
                                            "JPG (默认)",
                                        );
                                        ui.radio_value(
                                            &mut self.config.output_format,
                                            OutputFormat::KeepOriginal,
                                            "保持原始 (仅 PNG)",
                                        );
                                    });

                                    ui.add_space(15.0);
                                    ui.separator();
                                    ui.add_space(10.0);

                                    // 摄影级优化选项
                                    ui.label(
                                        egui::RichText::new("📷 摄影级优化")
                                            .size(14.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(37, 99, 235)),
                                    );
                                    ui.add_space(5.0);

                                    ui.horizontal(|ui| {
                                        let mut sharpening = self.config.enable_sharpening;
                                        ui.checkbox(&mut sharpening, "智能锐化");
                                        self.config.enable_sharpening = sharpening;

                                        if ui
                                            .button("ℹ️")
                                            .on_hover_text(
                                                "根据图片尺寸和内容智能锐化，避免过度处理",
                                            )
                                            .clicked()
                                        {
                                            // 可以显示更多信息
                                        }
                                    });

                                    ui.horizontal(|ui| {
                                        ui.label(
                                            egui::RichText::new("色彩空间:")
                                                .size(12.0)
                                                .color(egui::Color32::GRAY),
                                        );
                                        ui.label(
                                            egui::RichText::new(
                                                "≤3000px 自动转 sRGB，>3000px 保持原色域",
                                            )
                                            .size(11.0)
                                            .color(egui::Color32::GRAY),
                                        );
                                    });
                                });
                            });
                    }

                    ui.add_space(15.0);

                    // ===== 拖放区（处理中自动收小） =====
                    let available_width = ui.available_width();
                    let drop_h = if self.processing { 100.0 } else { 180.0 };
                    let (rect, response) = ui
                        .allocate_at_least(egui::vec2(available_width, drop_h), egui::Sense::click());

                    let is_hovering = (ctx.input(|i| !i.raw.hovered_files.is_empty())
                        || response.hovered())
                        && !self.processing;

                    let bg_color = if is_hovering {
                        egui::Color32::from_rgb(239, 246, 255)
                    } else {
                        egui::Color32::WHITE
                    };
                    let stroke_color = if is_hovering {
                        egui::Color32::from_rgb(37, 99, 235)
                    } else {
                        egui::Color32::from_rgb(226, 232, 240)
                    };
                    let stroke_width = if is_hovering { 2.5 } else { 1.5 };

                    ui.painter().rect(
                        rect,
                        egui::CornerRadius::same(16),
                        bg_color,
                        egui::Stroke::new(stroke_width, stroke_color),
                        egui::StrokeKind::Inside,
                    );

                    if self.processing {
                        // 处理中：仅一行提示，文字在暗框内上下左右居中
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "⏳ 正在处理…（可点下方按钮优雅停止）",
                            egui::FontId::proportional(13.0),
                            egui::Color32::from_rgb(100, 116, 139),
                        );
                    } else {
                        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(40.0);
                                ui.label(egui::RichText::new("📥").size(40.0));
                                ui.add_space(8.0);
                                ui.label(
                                    egui::RichText::new("拖入图片或文件夹，放开即自动开始")
                                        .size(16.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(30, 41, 59)),
                                );
                                ui.add_space(5.0);
                                ui.label(
                                    egui::RichText::new(
                                        "支持 JPG, PNG, WEBP, TIFF, DNG, RAW 等格式",
                                    )
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(100, 116, 139)),
                                );
                                ui.label(
                                    egui::RichText::new("支持整个文件夹，自动递归处理子目录")
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(148, 163, 184)),
                                );

                                ui.add_space(15.0);
                                ui.horizontal(|ui| {
                                    ui.add_space(ui.available_width() / 2.0 - 55.0);
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("浏览文件")
                                                    .color(egui::Color32::WHITE),
                                            )
                                            .fill(egui::Color32::from_rgb(37, 99, 235))
                                            .corner_radius(6.0),
                                        )
                                        .clicked()
                                    {
                                        if let Some(paths) = rfd::FileDialog::new()
                                            .add_filter(
                                                "图片文件",
                                                &[
                                                    "jpg", "jpeg", "png", "webp", "bmp", "tif",
                                                    "tiff", "dng", "cr2", "cr3", "nef", "arw",
                                                    "orf", "raf", "rw2", "pef", "srw",
                                                ],
                                            )
                                            .pick_files()
                                        {
                                            self.add_files(paths);
                                            self.start_processing();
                                        }
                                    }
                                });
                            });
                        });
                    }

                    if response.clicked() && !self.processing {
                        if let Some(paths) = rfd::FileDialog::new()
                            .add_filter(
                                "图片文件",
                                &[
                                    "jpg", "jpeg", "png", "webp", "bmp", "tif", "tiff", "dng",
                                    "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw",
                                ],
                            )
                            .pick_files()
                        {
                            self.add_files(paths);
                            self.start_processing();
                        }
                    }

                    ui.add_space(8.0);

                    if self.processing {
                        // 优雅停止按钮：当前图处理完才停，已输出图片保留
                        if !self.stop_requested {
                            ui.vertical_centered(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new("⏸ 优雅停止")
                                                .color(egui::Color32::WHITE),
                                        )
                                        .fill(egui::Color32::from_rgb(220, 38, 38))
                                        .corner_radius(6.0),
                                    )
                                    .clicked()
                                {
                                    self.stop_flag.store(true, Ordering::SeqCst);
                                    self.stop_requested = true;
                                }
                            });
                        }
                    } else {
                        if !self.files.is_empty() && ui.button("🚀 开始压缩").clicked() {
                            self.start_processing();
                        }

                        if !self.files.is_empty() && self.success_count > 0 {
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label(format!("✅ 成功处理 {} 个文件", self.success_count));
                                if ui.button("📂 打开输出文件夹").clicked() {
                                    if let Some(first) = self.files.front() {
                                        if let Some(dir) = first.path.parent() {
                                            let _ = opener::open(dir);
                                        }
                                    }
                                }
                            });
                        }
                    }

                    ui.add_space(10.0);
                });
            });

        if self.show_about {
            egui::Window::new("关于")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new("📸 图片高速压缩工具")
                                .size(20.0)
                                .strong(),
                        );
                        ui.add_space(10.0);
                        ui.label(format!("版本: {}", self.about_version));
                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(10.0);
                        ui.label("基于 Rust 高性能图片处理库开发");
                        ui.label("支持超大尺寸图片极速压缩");
                        ui.label("支持 RAW 格式 (DNG/CR2/NEF/ARW)");
                        ui.label("EXIF 元数据保留 | 路径自愈 | 内存优化");
                        ui.add_space(10.0);
                        ui.label("© 2026 星TAP实验室");
                        ui.add_space(15.0);
                        if ui.button("关闭").clicked() {
                            self.show_about = false;
                        }
                    });
                });
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let _ = save_config(&self.config);
    }
}

fn load_icon() -> Option<IconData> {
    match ::image::load_from_memory(include_bytes!("../icon.png")) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (w, h) = rgba.dimensions();
            Some(IconData {
                rgba: rgba.into_raw(),
                width: w,
                height: h,
            })
        }
        Err(_) => None,
    }
}

/// GUI 入口：rayon 池预热（封顶 4 线程防 OOM）+ eframe 启动
pub(crate) fn run_gui() -> Result<()> {
    // 预热 Rayon 全局线程池（仅一次），消除首次拖入时的卡顿
    // GUI 默认限制并行线程数（封顶 4），避免多张大图同时解码吃满内存导致 OOM
    let gui_workers = std::cmp::min(get(), 4);
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(gui_workers)
        .build_global();

    let icon_data = load_icon();

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size([540.0, 700.0])
        .with_title("星TAP 高清缩图")
        .with_resizable(false)
        .with_drag_and_drop(true);

    if let Some(icon) = icon_data {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "rust_image_compressor",
        options,
        Box::new(|cc| Ok(Box::new(ImageCompressorApp::new(cc)))),
    )
    .map_err(|e| anyhow::anyhow!("GUI error: {}", e))?;

    Ok(())
}
