#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;

use anyhow::Result;

// Windows 终端编码修正：统一 stdout/stderr 为 UTF-8，避免中文 log 在 GBK 终端显示乱码
#[cfg(target_os = "windows")]
fn fix_windows_console() {
    unsafe {
        extern "system" {
            fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
        }
        SetConsoleOutputCP(65001); // CP_UTF8
    }
}

#[cfg(not(target_os = "windows"))]
fn fix_windows_console() {} // no-op on non-Windows
use clap::Parser;
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use egui::IconData;
use num_cpus::get;
use rayon::prelude::*;
use std::collections::VecDeque;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use image::GenericImageView;

use cli::{
    build_capabilities, build_envelope, platform_preset, Cli, FileResult, JsonInput,
    PerceptualMetricsOut, StepTimings,
};
use rust_image_compressor::perceptual::{FocusMode, PerceptualMetrics, PerceptualOptions, QuantMode};
use rust_image_compressor::{
    app_config_to_process_config, AppConfig, ColorSpace, OutputFormat, ProcessMode, Processor,
};

/// 从 CLI 参数构造感知压缩选项（--perceptual 未开启时返回 None → 完全走 v4.1.0 旧路径）
fn perceptual_options_from_cli(cli: &Cli) -> Option<PerceptualOptions> {
    if !cli.perceptual {
        return None;
    }
    Some(PerceptualOptions {
        denoise_strength: cli.denoise_strength.min(100),
        focus_mode: cli.focus_mode.into(),
        quant_mode: cli.quant_mode.into(),
        quality_ceil: cli.quality_ceil.unwrap_or(95),
        platform: cli.platform.map(|p| p.as_str().to_string()),
        ..Default::default()
    })
}

fn get_config_file_path() -> Result<PathBuf> {
    if let Some(mut path) = dirs::config_dir() {
        path.push("rust_image_compressor");
        fs::create_dir_all(&path)?;
        path.push("config.toml");
        Ok(path)
    } else {
        Ok(PathBuf::from("image_compressor_config.toml"))
    }
}

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
            about_version: "v4.0.8".to_string(),
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
    }

    fn start_processing(&mut self) {
        if self.files.is_empty() || self.processing {
            return;
        }

        self.processing = true;
        self.processed_count = 0;
        self.success_count = 0;

        let files: Vec<FileItem> = self.files.clone().into_iter().collect();
        let config = self.config.clone();
        let custom_output_dir = self.custom_output_dir.clone();
        let tx = self.tx.clone();

        let _ = std::thread::spawn(move || {
            let processor_config = app_config_to_process_config(&config, custom_output_dir);
            let processor = Processor::new(processor_config);
            let total = files.len();
            let success_count = AtomicUsize::new(0);

            // 使用并行迭代器，但每个任务完成后立即发送进度
            files.par_iter().enumerate().for_each(|(index, item)| {
                let result = processor.process_image(&item.path);
                let is_success = result.is_ok();

                if is_success {
                    success_count.fetch_add(1, Ordering::Relaxed);
                }

                // 立即发送进度更新（使用索引）
                let _ = tx.send(AppEvent::ProcessingProgress(
                    index,
                    if is_success { 1 } else { 0 },
                ));
            });

            let _ = tx.send(AppEvent::ProcessingFinished(
                total,
                success_count.load(Ordering::Relaxed),
            ));
        });
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
                    self.processed_count = total;
                    self.success_count = success;
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
                            ui.label(
                                egui::RichText::new(format!(
                                    "正在处理 {} 个文件...",
                                    self.processed_count
                                ))
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
                        if self.processed_count > 0 {
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
                    ui.add_space(10.0);
                    ui.label(
                        egui::RichText::new("星TAP 实验室 | 高性能 Rust 内核 v4.0")
                            .size(10.0)
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
                                        egui::RichText::new("✨ 内核升级 v4.0")
                                            .strong()
                                            .color(egui::Color32::from_rgb(37, 99, 235)),
                                    );
                                    ui.add_space(10.0);
                                    ui.label(
                                        egui::RichText::new(
                                            "LTO 全局优化 | EXIF 保留 | 路径自愈 | 内存优化",
                                        )
                                        .size(11.0)
                                        .color(egui::Color32::from_rgb(100, 116, 139)),
                                    );
                                });
                                ui.add_space(10.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("选择输出模式")
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
                                ui.horizontal(|ui| {
                                    ui.radio_value(
                                        &mut self.config.mode,
                                        ProcessMode::WeChat,
                                        "微信优化 (900KB)",
                                    );
                                    ui.add_space(15.0);
                                    ui.radio_value(
                                        &mut self.config.mode,
                                        ProcessMode::HD,
                                        "高清无损 (5MB)",
                                    );
                                    ui.add_space(15.0);
                                    ui.radio_value(
                                        &mut self.config.mode,
                                        ProcessMode::Custom,
                                        "自定义模式",
                                    );
                                });

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

                    if self.show_advanced {
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
                                                "数值越高越清晰、体积越大（微信/高清模式已固定）",
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

                    let available_width = ui.available_width();
                    let (rect, response) = ui.allocate_at_least(
                        egui::vec2(available_width, 180.0),
                        egui::Sense::click(),
                    );

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

                    ui.allocate_new_ui(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(40.0);
                            ui.label(egui::RichText::new("📥").size(40.0));
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("拖入图片或文件夹")
                                    .size(16.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(30, 41, 59)),
                            );
                            ui.add_space(5.0);
                            ui.label(
                                egui::RichText::new("支持 JPG, PNG, WEBP, DNG, RAW 等格式")
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
                                                "jpg", "jpeg", "png", "webp", "bmp", "dng", "cr2",
                                                "cr3", "nef", "arw", "orf", "raf", "rw2", "pef",
                                                "srw",
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

                    if response.clicked() && !self.processing {
                        if let Some(paths) = rfd::FileDialog::new().pick_files() {
                            self.add_files(paths);
                            self.start_processing();
                        }
                    }

                    if !self.files.is_empty()
                        && !self.processing
                        && ui.button("🚀 开始压缩").clicked()
                    {
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

fn main() -> Result<()> {
    // Windows 终端 UTF-8 编码修正（首行执行，避免中文 log 乱码）
    fix_windows_console();

    // 用 args_os 判断,避免 Windows 上含非 UTF-8 路径时 panic
    if std::env::args_os().count() > 1 {
        let cli = Cli::parse();

        // 并行并发数：AI 可经 --max-workers 限流（在全局池首次使用前生效）
        apply_max_workers(cli.max_workers);

        // --capabilities 优先：输出版本支持的全部参数 schema
        if cli.capabilities {
            let caps = build_capabilities();
            println!("{}", serde_json::to_string_pretty(&caps)?);
            return Ok(());
        }

        // --self-check：环境自检，内置测试图走完整管线，输出健康报告后退出
        if cli.self_check {
            return run_self_check();
        }

        // --json-in 优先：直接传 JSON 字符串，AI 最稳的用法
        if let Some(json_str) = &cli.json_in {
            let json_input: JsonInput = serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("--json-in 解析失败: {}", e))?;
            return run_json_mode(&json_input);
        }

        return run_cli(&cli);
    }

    // 预热 Rayon 全局线程池（仅一次），消除首次拖入时的卡顿
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(get())
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

fn run_cli(cli: &Cli) -> Result<()> {
    // 先收集文件列表(目录默认递归;记录被拒路径用于提示)
    let mut files = Vec::new();
    let mut rejected: Vec<(String, String)> = Vec::new();
    let recursive = cli.recursive.unwrap_or(!cli.no_recursive);

    // 处理显式的--input参数
    for input_path in &cli.input {
        classify_input(input_path, &mut files, &mut rejected, recursive);
    }

    // 处理SendTo传递的位置参数
    for input_path in &cli.positional {
        classify_input(input_path, &mut files, &mut rejected, recursive);
    }

    // 应用 include/exclude 过滤
    if let Some(ref include_glob) = cli.include {
        let patterns: Vec<&str> = include_glob.split(',').map(|s| s.trim()).collect();
        files.retain(|f| match_patterns(f, &patterns));
    }
    if let Some(ref exclude_glob) = cli.exclude {
        let patterns: Vec<&str> = exclude_glob.split(',').map(|s| s.trim()).collect();
        files.retain(|f| !match_patterns(f, &patterns));
    }

    // 如果是 JSON 模式
    if cli.json {
        // 先尝试从 stdin 读取 JSON
        let mut stdin_input = String::new();
        let stdin_result = std::io::stdin().read_to_string(&mut stdin_input);

        if stdin_result.is_ok() && !stdin_input.trim().is_empty() {
            // 从 stdin 读到了 JSON，用 JSON 模式
            let json_input: JsonInput = serde_json::from_str(&stdin_input)?;
            return run_json_mode(&json_input);
        } else {
            // 没有从 stdin 读到 JSON，用 CLI 参数，但输出 JSON 格式
            if files.is_empty() && (!cli.input.is_empty() || !cli.positional.is_empty()) {
                eprintln!("\n⚠️ 没有可处理的文件。你给出的路径均无效：");
                for (p, why) in &rejected {
                    eprintln!("   • {} —— {}", p, why);
                }
                print_cli_hint();
                std::process::exit(2);
            }
            return run_cli_with_json_output(cli, &files);
        }
    }

    // 普通 CLI 模式：输入无效时给出明确提示并退出(非 0),避免 AI/脚本误判为成功
    if files.is_empty() && (!cli.input.is_empty() || !cli.positional.is_empty()) {
        eprintln!("\n⚠️ 没有处理任何文件。你给出的路径均无效：");
        for (p, why) in &rejected {
            eprintln!("   • {} —— {}", p, why);
        }
        print_cli_hint();
        std::process::exit(2);
    }

    if files.is_empty() {
        // 完全没给参数：提示用法
        eprintln!("未指定任何文件/目录。用 --help 查看用法。");
        print_cli_hint();
        std::process::exit(2);
    }

    // --dry-run 预演模式：只输出文件列表和配置，不压缩
    if cli.dry_run {
        if cli.quiet {
            // 静默 dry-run：用标准信封格式（data.results 数组），AI 零分支解析
            let results: Vec<FileResult> = files
                .iter()
                .map(|f| FileResult {
                    input: f.display().to_string().replace('\\', "/"),
                    success: true,
                    ..Default::default()
                })
                .collect();
            let envelope = build_envelope(&results, std::time::Instant::now());
            println!("{}", serde_json::to_string(&envelope)?);
        } else {
            println!("\n🔍 [预演模式] 以下文件将被处理（未执行压缩）：");
            println!("   文件数: {}", files.len());
            println!("   模式:   {:?}", cli.mode);
            println!("   质量:   {}", cli.quality);
            println!("   长边:   {}", cli.max_dim);
            let output_dir_display = cli
                .output_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "./compressed/".to_string());
            println!("   输出到: {}", output_dir_display);
            println!(
                "   锐化:   {}",
                if cli.enable_sharpening { "开" } else { "关" }
            );
            println!();
            for f in &files {
                println!("   📄 {}", f.display());
            }
            println!("\n✅ dry-run 完成，共 {} 个文件", files.len());
        }
        return Ok(());
    }

    // A/B 对照 / 基准对比模式：旧路径(v4.1.0) vs 新感知路径 双输出 + 对比表
    if cli.ab || cli.benchmark {
        return run_compare_mode(cli, &files);
    }

    let app_config = cli.to_app_config();

    // CLI/AI 模式默认输出到 ./compressed/，不污染源目录
    let effective_output_dir = cli
        .output_dir
        .clone()
        .map(|p| {
            if p.is_relative() {
                std::env::current_dir().unwrap_or_default().join(&p)
            } else {
                p
            }
        })
        .or_else(|| {
            Some(
                std::env::current_dir()
                    .unwrap_or_default()
                    .join("compressed"),
            )
        });
    let mut process_config = app_config_to_process_config(&app_config, effective_output_dir);
    process_config.perceptual = perceptual_options_from_cli(cli);
    let processor = Processor::new(process_config);

    // 多核并行处理，充分利用 CPU（统一走 process_one_file，支持幂等续跑/JSONL）
    let quiet = cli.quiet;
    let force = cli.force;
    let overwrite = cli.overwrite;
    let jsonl = cli.jsonl;
    let results: Vec<FileResult> = files
        .par_iter()
        .map(|file| {
            let r = process_one_file(&processor, file, force, overwrite);
            if jsonl {
                emit_jsonl(&r);
            }
            r
        })
        .collect();

    let completed = results.iter().filter(|r| r.success).count();
    let failed = results.len() - completed;

    if !quiet && !jsonl {
        for r in &results {
            if r.success {
                println!("  ✅ Success: {}", r.output.clone().unwrap_or_default());
            } else {
                println!("  ❌ Failed: {}", r.error.clone().unwrap_or_default());
            }
        }
        println!("\n✅ 处理完成！成功: {}, 失败: {}", completed, failed);
    }

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// CLI 输入无效时的通用「第一次用」避坑提示(给人类与 AI 看)
fn print_cli_hint() {
    eprintln!("\n💡 正确用法(避坑):");
    eprintln!("   1) 路径含空格/中文,必须用引号包住整个路径:");
    eprintln!("        Windows(cmd): 图片高速压缩.exe \"C:\\用户\\张三\\a b.jpg\"");
    eprintln!("        macOS/Linux:  ./图片高速压缩 \"~/图片/我的照片.jpg\"");
    eprintln!("      未加引号会被 shell 按空格拆成多段 → 找不到文件。");
    eprintln!("   2) 目录默认递归处理子目录;空目录会得到 0 个文件。");
    eprintln!("   3) RAW(.cr3/.nef/.arw…) 仅在 macOS 支持;Windows 请先转 JPG/PNG。");
    eprintln!("   4) 给 AI/脚本最稳:用 --json 从 stdin 传路径数组(绕开 shell 分词);");
    eprintln!("      或 Python subprocess 用列表传参(走 Unicode 命令行)。");
    eprintln!();
}

/// 把命令行输入的路径分类:目录(递归收集)/受支持图片/格式不符/不存在
fn classify_input(
    input_path: &Path,
    files: &mut Vec<PathBuf>,
    rejected: &mut Vec<(String, String)>,
    recursive: bool,
) {
    if input_path.is_dir() {
        collect_images(input_path, files, recursive, 0, rejected);
    } else if input_path.is_file() {
        if is_supported_image(input_path) {
            files.push(input_path.to_path_buf());
        } else {
            rejected.push((
                input_path.display().to_string(),
                "不是支持的图片格式".to_string(),
            ));
        }
    } else {
        let hint = if input_path.to_string_lossy().contains(' ') {
            "(路径含空格却没加引号?shell 会按空格拆成多段导致找不到文件 —— 请用引号包住整个路径)"
        } else {
            ""
        };
        rejected.push((
            input_path.display().to_string(),
            format!("路径不存在或无法访问{}", hint),
        ));
    }
}

/// 递归收集目录下所有受支持图片(深度上限 20,防止符号链接环)
fn collect_images(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    recursive: bool,
    depth: usize,
    _rejected: &mut Vec<(String, String)>,
) {
    if depth > 20 {
        return;
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    if recursive {
                        collect_images(&p, out, recursive, depth + 1, _rejected);
                    }
                } else if meta.is_file() && is_supported_image(&p) {
                    out.push(p);
                }
            }
        }
    }
}

/// 用 CLI 参数处理，但输出标准 JSON 信封格式
fn run_cli_with_json_output(cli: &Cli, files: &[PathBuf]) -> Result<()> {
    let start = std::time::Instant::now();
    let app_config = cli.to_app_config();

    // CLI/AI 模式默认输出到 ./compressed/
    let effective_output_dir = cli
        .output_dir
        .clone()
        .map(|p| {
            if p.is_relative() {
                std::env::current_dir().unwrap_or_default().join(&p)
            } else {
                p
            }
        })
        .or_else(|| {
            Some(
                std::env::current_dir()
                    .unwrap_or_default()
                    .join("compressed"),
            )
        });
    let mut process_config = app_config_to_process_config(&app_config, effective_output_dir);
    process_config.perceptual = perceptual_options_from_cli(cli);
    let processor = Processor::new(process_config);

    let force = cli.force;
    let jsonl = cli.jsonl;
    let overwrite = cli.overwrite;
    let results: Vec<FileResult> = files
        .par_iter()
        .map(|file| {
            let result = process_one_file(&processor, file, force, overwrite);
            if jsonl {
                // 流式 JSONL：每处理完一个文件立即输出一行（println! 自带行级锁）
                if let Ok(line) = serde_json::to_string(&result) {
                    println!("{}", line);
                }
            }
            result
        })
        .collect();

    let envelope = build_envelope(&results, start);
    println!("{}", serde_json::to_string(&envelope)?);

    // 有失败时退出码 1，让 AI 脚本能检测
    if envelope.data.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// 由 Processor 的感知配置 + 实测指标 组装 JSON 输出用的感知指标块（perceptual=None 且无指标时返回 None）
fn build_perceptual_out(
    processor: &Processor,
    metrics: Option<&PerceptualMetrics>,
) -> Option<PerceptualMetricsOut> {
    let cfg = processor.perceptual_config();
    let mode = cfg.is_some();
    if !mode && metrics.is_none() {
        return None;
    }
    let budget = processor.effective_target_kb();
    Some(PerceptualMetricsOut {
        perceptual_mode: mode,
        platform: cfg.and_then(|c| c.platform.clone()),
        quant_mode: cfg.map(|c| c.quant_mode.as_str().to_string()),
        denoise_strength: cfg.map(|c| c.denoise_strength),
        focus_mode: cfg.map(|c| match c.focus_mode {
            FocusMode::Auto => "auto".to_string(),
            FocusMode::Center => "center".to_string(),
        }),
        bytes_budget_kb: if budget > 0 { Some(budget as f64) } else { None },
        ssim_vs_source: metrics.map(|m| m.ssim_vs_source),
        psnr_vs_source: metrics.map(|m| m.psnr_vs_source),
        final_quality: metrics.map(|m| m.final_quality),
        step_timings: metrics.map(|m| StepTimings {
            denoise_ms: m.denoise_ms,
            downscale_ms: m.downscale_ms,
            sharpen_ms: m.sharpen_ms,
            encode_ms: m.encode_ms,
        }),
    })
}

/// 单文件处理（CLI-JSON / AI-JSON 两条通路共用）：
/// 幂等续跑（输出已存在且未 force 则跳过）+ 路径归一化 + 完整指标
fn process_one_file(
    processor: &Processor,
    file: &Path,
    force: bool,
    overwrite: bool,
) -> FileResult {
    let file_str = file.display().to_string().replace('\\', "/");

    // 幂等续跑：输出已存在且未 force → 跳过（success=true, skipped=true）
    if !force && !overwrite {
        let expected = processor.expected_output_path(file);
        if expected.exists() {
            let original_size = fs::metadata(file).ok().map(|m| m.len());
            let compressed_size = fs::metadata(&expected).ok().map(|m| m.len());
            let compression_ratio = match (original_size, compressed_size) {
                (Some(o), Some(c)) if c > 0 => Some(o as f64 / c as f64),
                _ => None,
            };
            eprintln!("[INFO] 跳过（输出已存在，force 可强制重压）: {}", file_str);
            return FileResult {
                input: file_str,
                output: Some(expected.display().to_string().replace('\\', "/")),
                success: true,
                error: None,
                original_size,
                compressed_size,
                compression_ratio,
                skipped: Some(true),
                perceptual: build_perceptual_out(processor, None),
            };
        }
    }

    let original_size = fs::metadata(file).ok().map(|m| m.len());
    let (success, output, error, percept) = match processor.process_image_with_metrics(file) {
        Ok((output_path, m)) => (true, Some(output_path.display().to_string()), None, m),
        Err(e) => (false, None, Some(e.to_string()), None),
    };

    let compressed_size = output
        .as_ref()
        .and_then(|p| fs::metadata(Path::new(p)).ok().map(|m| m.len()));
    let compression_ratio = match (original_size, compressed_size) {
        (Some(o), Some(c)) if c > 0 => Some(o as f64 / c as f64),
        _ => None,
    };

    let perceptual = build_perceptual_out(processor, percept.as_ref());

    FileResult {
        input: file_str,
        output: output.map(|p| p.replace('\\', "/")),
        success,
        error,
        original_size,
        compressed_size,
        compression_ratio,
        skipped: None,
        perceptual,
    }
}

/// 设置 Rayon 全局并行线程数（仅在尚未初始化全局池时生效；重复设置被忽略）
fn apply_max_workers(n: Option<usize>) {
    if let Some(n) = n {
        if n > 0 {
            let _ = rayon::ThreadPoolBuilder::new()
                .num_threads(n)
                .build_global();
        }
    }
}

/// 流式 JSONL：单行输出一个文件结果（println! 自带行级锁，跨线程安全）
fn emit_jsonl(r: &FileResult) {
    if let Ok(line) = serde_json::to_string(r) {
        println!("{}", line);
    }
}

/// A/B 对照 / 基准对比：同一图分别跑旧路径(v4.1.0)与新感知路径，
/// 输出 old/new 对照图 + 并排 montage（--ab），打印 体积/SSIM/PSNR/各步耗时 对比表（--benchmark）
fn run_compare_mode(cli: &Cli, files: &[PathBuf]) -> Result<()> {
    let app_config = cli.to_app_config();
    let out_base = cli
        .output_dir
        .clone()
        .map(|p| {
            if p.is_relative() {
                std::env::current_dir().unwrap_or_default().join(&p)
            } else {
                p
            }
        })
        .or_else(|| Some(std::env::current_dir().unwrap_or_default().join("compressed")))
        .unwrap_or_default();
    let ab_dir = out_base.join("ab_output");
    let old_dir = ab_dir.join("old");
    let new_dir = ab_dir.join("new");
    let _ = fs::create_dir_all(&old_dir);
    let _ = fs::create_dir_all(&new_dir);

    // 旧路径：perceptual=None，完全 v4.1.0 旧行为
    let mut old_cfg = app_config_to_process_config(&app_config, Some(old_dir.clone()));
    old_cfg.perceptual = None;
    let old_proc = Processor::new(old_cfg);

    // 新路径：感知压缩
    let mut new_cfg = app_config_to_process_config(&app_config, Some(new_dir.clone()));
    new_cfg.perceptual = perceptual_options_from_cli(cli);
    let new_proc = Processor::new(new_cfg);

    let force = cli.force;
    let overwrite = cli.overwrite;

    if cli.benchmark {
        println!("\n=== 感知压缩 A/B 基准对比（旧 v4.1.0 vs 新感知路径）===");
        println!(
            "{:<26} {:>9} {:>9} {:>9} {:>9} {:>7}",
            "file", "old_KB", "new_KB", "oldSSIM", "newSSIM", "newQ"
        );
    }

    let mut montage_rows: Vec<String> = Vec::new();
    for file in files {
        let old_r = process_one_file(&old_proc, file, force, overwrite);
        let new_r = process_one_file(&new_proc, file, force, overwrite);

        let old_size = old_r.compressed_size.unwrap_or(0);
        let new_size = new_r.compressed_size.unwrap_or(0);

        // 旧/新 各自与源图（降采样后）的 SSIM，外部核算、算法同源
        let old_ssim = old_r
            .output
            .as_ref()
            .and_then(|p| ssim_psnr_vs_source(file, Path::new(p)))
            .map(|(s, _)| s)
            .unwrap_or(f64::NAN);
        let new_ssim = new_r
            .output
            .as_ref()
            .and_then(|p| ssim_psnr_vs_source(file, Path::new(p)))
            .map(|(s, _)| s)
            .unwrap_or(f64::NAN);
        let new_q = new_r
            .perceptual
            .as_ref()
            .and_then(|m| m.final_quality)
            .unwrap_or(0);

        if cli.benchmark {
            println!(
                "{:<26} {:>9.1} {:>9.1} {:>9.4} {:>9.4} {:>7}",
                file.file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default(),
                old_size as f64 / 1024.0,
                new_size as f64 / 1024.0,
                old_ssim,
                new_ssim,
                new_q
            );
        }

        // 并排 montage（--ab）
        if cli.ab {
            if let (Some(op), Some(np)) = (old_r.output.as_ref(), new_r.output.as_ref()) {
                let stem = file
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_else(|| "image".to_string());
                let montage_path = ab_dir.join(format!("ab_{}.jpg", stem));
                if make_montage(Path::new(op), Path::new(np), &montage_path) {
                    montage_rows.push(format!(
                        "  📊 {} → ab_output/ab_{}.jpg（左=旧 v4.1.0，右=新感知）",
                        stem, stem
                    ));
                }
            }
        }
    }

    if cli.ab {
        println!("\n✅ A/B 对照图已输出到: {}", ab_dir.display());
        for r in &montage_rows {
            println!("{}", r);
        }
    }
    if cli.benchmark {
        println!("=== 对比结束（盲测请放大 200% 看睫毛/暗部）===\n");
    }

    Ok(())
}

/// 与源图（降采样到输出尺寸）对齐的 SSIM/PSNR，算法与工具内部 to_gray/ssim_gray 同源
fn ssim_psnr_vs_source(orig: &Path, out: &Path) -> Option<(f64, f64)> {
    let orig_img = image::open(orig).ok()?;
    let out_img = image::open(out).ok()?;
    let (ow, oh) = out_img.dimensions();
    let orig_resized =
        image::imageops::resize(&orig_img.to_rgb8(), ow, oh, image::imageops::FilterType::Triangle);
    let orig_dyn = image::DynamicImage::ImageRgb8(orig_resized);
    let (ref_gray, _, _) = rust_image_compressor::perceptual::to_gray(&orig_dyn);
    let (out_gray, gw, gh) = rust_image_compressor::perceptual::to_gray(&out_img);
    if gw != ow as usize || gh != oh as usize {
        return None;
    }
    let ssim = rust_image_compressor::perceptual::ssim_gray(&ref_gray, &out_gray, gw, gh);
    let psnr = rust_image_compressor::perceptual::psnr_gray(&ref_gray, &out_gray);
    Some((ssim, psnr))
}

/// 左右并排 montage（中间 4px 白缝），用于 A/B 对照
fn make_montage(left: &Path, right: &Path, out: &Path) -> bool {
    let l = match image::open(left) {
        Ok(i) => i,
        Err(_) => return false,
    };
    let r = match image::open(right) {
        Ok(i) => i,
        Err(_) => return false,
    };
    let (lw, lh) = l.dimensions();
    let (rw, rh) = r.dimensions();
    let h = lh.min(rh);
    let lw2 = ((lw as f64 * h as f64 / lh as f64) as u32).max(1);
    let rw2 = ((rw as f64 * h as f64 / rh as f64) as u32).max(1);
    let gap = 4u32;
    let total_w = lw2 + gap + rw2;
    let mut canvas = image::RgbImage::new(total_w, h);
    for p in canvas.pixels_mut() {
        *p = image::Rgb([255u8, 255u8, 255u8]);
    }
    let l_resized =
        image::imageops::resize(&l.to_rgb8(), lw2, h, image::imageops::FilterType::Triangle);
    image::imageops::replace(&mut canvas, &l_resized, 0, 0);
    let r_resized =
        image::imageops::resize(&r.to_rgb8(), rw2, h, image::imageops::FilterType::Triangle);
    image::imageops::replace(&mut canvas, &r_resized, (lw2 + gap) as i64, 0);
    canvas.save(out).is_ok()
}

/// 生成一张带渐变的测试图（避免纯色被编码优化成极小体积，导致自检失真）
fn generate_test_image(width: u32, height: u32) -> image::RgbImage {
    let mut img = image::RgbImage::new(width, height);
    for y in 0..height {
        for x in 0..width {
            let r = (x * 255 / width) as u8;
            let g = (y * 255 / height) as u8;
            let b = ((x + y) * 255 / (width + height)) as u8;
            img.put_pixel(x, y, image::Rgb([r, g, b]));
        }
    }
    img
}

/// 环境自检：内置生成测试图，完整走一遍压缩管线，输出健康报告后退出
fn run_self_check() -> Result<()> {
    let start = std::time::Instant::now();
    eprintln!("[INFO] 开始环境自检：生成测试图并走完整压缩管线...");

    // 1) 生成测试图并落盘为临时输入
    let width = 1024u32;
    let height = 768u32;
    let img = generate_test_image(width, height);
    let tmp = std::env::temp_dir().join("rust_image_compressor_selfcheck");
    fs::create_dir_all(&tmp)?;
    let input_path = tmp.join("selfcheck_source.png");
    image::DynamicImage::ImageRgb8(img)
        .save(&input_path)
        .map_err(|e| anyhow::anyhow!("测试图保存失败: {}", e))?;
    let original_size = fs::metadata(&input_path).ok().map(|m| m.len()).unwrap_or(0);
    eprintln!(
        "[INFO] 生成测试图 {}x{} ({} bytes)",
        width, height, original_size
    );

    // 2) 配置 + 处理（默认 custom + quality 85 + 长边 1000）
    let app_config = AppConfig {
        custom_quality: 85,
        custom_max_dim: 1000,
        ..Default::default()
    };
    let output_dir = tmp.join("out");
    let process_config = app_config_to_process_config(&app_config, Some(output_dir.clone()));
    let processor = Processor::new(process_config);
    let result = process_one_file(&processor, &input_path, true, false);

    // 3) 逐项校验
    let ok_pipeline = result.success && result.output.is_some();
    let compressed_size = result.compressed_size.unwrap_or(0);
    let ok_size = compressed_size > 0 && compressed_size <= original_size;
    let ok_decode = match &result.output {
        Some(p) => image::open(Path::new(p)).is_ok(),
        None => false,
    };

    let checks = serde_json::json!([
        {
            "name": "pipeline",
            "passed": ok_pipeline,
            "detail": if ok_pipeline { result.output.clone().unwrap_or_default() } else { result.error.clone().unwrap_or_default() }
        },
        {
            "name": "output_size",
            "passed": ok_size,
            "detail": format!("原始 {} bytes → 压缩 {} bytes", original_size, compressed_size)
        },
        {
            "name": "decode_output",
            "passed": ok_decode,
            "detail": if ok_decode { "压缩图可正常解码".to_string() } else { "无法解码压缩图".to_string() }
        }
    ]);

    let all_passed = ok_pipeline && ok_size && ok_decode;
    let ratio = if compressed_size > 0 {
        original_size as f64 / compressed_size as f64
    } else {
        0.0
    };
    let report = serde_json::json!({
        "schema_version": "1.0",
        "command": "self-check",
        "status": if all_passed { "succeeded" } else { "failed" },
        "version": env!("CARGO_PKG_VERSION"),
        "elapsed_ms": start.elapsed().as_millis(),
        "checks": checks,
        "metrics": {
            "original_bytes": original_size,
            "compressed_bytes": compressed_size,
            "bytes_saved": original_size.saturating_sub(compressed_size),
            "ratio": ratio
        }
    });
    println!("{}", serde_json::to_string_pretty(&report)?);

    // 4) 清理临时目录（best-effort）
    let _ = fs::remove_dir_all(&tmp);

    if all_passed {
        eprintln!("[INFO] ✅ 环境自检通过");
        Ok(())
    } else {
        eprintln!("[ERROR] ❌ 环境自检未通过，请检查二进制/依赖");
        std::process::exit(1);
    }
}

fn run_json_mode(json_input: &JsonInput) -> Result<()> {
    let start = std::time::Instant::now();
    let mut app_config = AppConfig::default();

    if let Some(mode_str) = &json_input.mode {
        match mode_str.to_lowercase().as_str() {
            "wechat" => app_config.mode = ProcessMode::WeChat,
            "hd" => app_config.mode = ProcessMode::HD,
            "custom" => app_config.mode = ProcessMode::Custom,
            _ => {}
        }
    }

    if let Some(q) = json_input.quality {
        app_config.custom_quality = q;
    }
    if let Some(d) = json_input.max_dim {
        app_config.custom_max_dim = d;
    }
    if let Some(k) = json_input.target_kb {
        app_config.custom_target_kb = k;
    }
    if let Some(o) = json_input.overwrite {
        app_config.overwrite = o;
    }
    if let Some(k) = json_input.keep_original_name {
        app_config.keep_original_name = k;
    }
    if let Some(f) = &json_input.output_format {
        match f.to_lowercase().as_str() {
            "jpeg" | "jpg" => app_config.output_format = OutputFormat::Jpeg,
            "original" | "keep" => app_config.output_format = OutputFormat::KeepOriginal,
            _ => {}
        }
    }

    // 摄影级优化参数
    if let Some(v) = json_input.enable_sharpening {
        app_config.enable_sharpening = v;
    }
    if let Some(v) = json_input.sharpening_radius {
        app_config.sharpening_radius = v;
    }
    if let Some(v) = json_input.sharpening_amount {
        app_config.sharpening_amount = v;
    }
    if let Some(v) = json_input.use_custom_quantization {
        app_config.use_custom_quantization = v;
    }
    if let Some(v) = json_input.preserve_high_frequency {
        app_config.preserve_high_frequency = v;
    }
    if let Some(cs) = &json_input.color_space {
        match cs.to_lowercase().as_str() {
            "srgb" | "convert" => app_config.color_space = ColorSpace::ConvertToSRGB,
            _ => app_config.color_space = ColorSpace::KeepOriginal,
        }
    }

    // 平台阈值预设（§2）+ 体积线覆盖（与 CLI 同逻辑）
    if let Some(plat) = &json_input.platform {
        if let Some((md, q, kb, srgb)) = platform_preset(plat) {
            app_config.custom_max_dim = md;
            app_config.custom_quality = q;
            app_config.custom_target_kb = kb;
            if srgb {
                app_config.color_space = ColorSpace::ConvertToSRGB;
            }
        }
    }
    if let Some(kb) = json_input.target_budget_kb {
        app_config.custom_target_kb = kb;
    }

    // 输出目录：未指定时默认 ./compressed/，不污染源目录
    let output_dir = json_input
        .output_dir
        .as_ref()
        .map(|p| {
            let pb = PathBuf::from(p);
            if pb.is_relative() {
                std::env::current_dir().unwrap_or_default().join(&pb)
            } else {
                pb
            }
        })
        .or_else(|| {
            Some(
                std::env::current_dir()
                    .unwrap_or_default()
                    .join("compressed"),
            )
        });

    // 预演模式：只输出文件列表和配置，不压缩（标准信封格式）
    let is_dry_run = json_input.dry_run.unwrap_or(false);
    if is_dry_run {
        let dry_files = expand_file_list(&json_input.files);
        // dry-run 同样执行存在性/可读性校验，与真实执行 Schema 完全对齐
        let results: Vec<FileResult> = dry_files
            .iter()
            .map(|f| {
                let p = Path::new(f);
                let (success, error) = if !p.exists() || !p.is_file() {
                    (false, Some(format!("路径不存在或不是文件: {}", f)))
                } else if !is_supported_image(p) {
                    (false, Some(format!("不支持的图片格式: {}", f)))
                } else {
                    (true, None)
                };
                FileResult {
                    input: f.replace('\\', "/"),
                    success,
                    error,
                    original_size: fs::metadata(p).ok().map(|m| m.len()),
                    ..Default::default()
                }
            })
            .collect();
        let envelope = build_envelope(&results, std::time::Instant::now());
        println!("{}", serde_json::to_string(&envelope)?);
        return Ok(());
    }

    // P0-FIX: 展开目录 → 得到完整文件列表（目录自动递归扫描内部图片）
    let all_entries = expand_file_list(&json_input.files);
    let total_entries = all_entries.len();

    if total_entries == 0 {
        eprintln!("[ERROR] 没有可处理的文件");
        let envelope = build_envelope(&[], start);
        println!("{}", serde_json::to_string(&envelope)?);
        std::process::exit(1);
    }

    // 校验输出目录
    if let Some(ref dir) = output_dir {
        if let Some(parent) = dir.parent() {
            if !parent.exists() {
                eprintln!("[WARN] 输出目录的父目录不存在: {}", dir.display());
            }
        }
    }

    // 并行并发数：AI 可经 max_workers 限流（仅在全局池尚未初始化时生效）
    apply_max_workers(json_input.max_workers);

    let mut process_config = app_config_to_process_config(&app_config, output_dir);
    // 感知压缩选项（perceptual=false 或缺失 → None → 完全走 v4.1.0 旧路径，100% 兼容）
    process_config.perceptual = if json_input.perceptual.unwrap_or(false) {
        Some(PerceptualOptions {
            denoise_strength: json_input.denoise_strength.unwrap_or(25).min(100),
            focus_mode: match json_input.focus_mode.as_deref() {
                Some("center") => FocusMode::Center,
                _ => FocusMode::Auto,
            },
            // JSON 不暴露 quant_mode，默认 CSF（最稳）
            quant_mode: QuantMode::Csf,
            quality_ceil: json_input.quality_ceil.unwrap_or(95),
            budget_kb: json_input.target_budget_kb,
            platform: json_input.platform.clone(),
        })
    } else {
        None
    };
    let processor = Processor::new(process_config);

    let force = json_input.force.unwrap_or(false);
    let overwrite = app_config.overwrite;
    let jsonl = json_input.jsonl.unwrap_or(false);

    // P0-FIX: 所有输入条目保留在 results 内（不存在或格式不符自动标记失败）
    let results: Vec<FileResult> = all_entries
        .par_iter()
        .map(|entry| {
            let path = Path::new(entry);

            // 路径不存在 → 标记失败，保留结果到 results
            if !path.exists() || !path.is_file() {
                let r = FileResult {
                    input: entry.replace('\\', "/"),
                    success: false,
                    error: Some(format!("路径不存在或不是文件: {}", entry)),
                    ..Default::default()
                };
                if jsonl {
                    emit_jsonl(&r);
                }
                return r;
            }

            // 不支持的格式 → 标记失败
            if !is_supported_image(path) {
                let r = FileResult {
                    input: entry.replace('\\', "/"),
                    success: false,
                    error: Some(format!("不支持的图片格式: {}", entry)),
                    ..Default::default()
                };
                if jsonl {
                    emit_jsonl(&r);
                }
                return r;
            }

            // 正常处理（process_one_file 内含幂等续跑跳过逻辑）
            let r = process_one_file(&processor, path, force, overwrite);
            if jsonl {
                emit_jsonl(&r);
            }
            r
        })
        .collect();

    let envelope = build_envelope(&results, start);
    println!("{}", serde_json::to_string(&envelope)?);

    // P0-FIX: 存在任意失败即退出码 1
    if envelope.data.failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

/// 展开文件数组：自动识别目录并递归扫描内部图片
fn expand_file_list(files: &[String]) -> Vec<String> {
    let mut entries = Vec::new();
    for f in files {
        let path = Path::new(f);
        if path.is_dir() {
            let mut scanned = Vec::new();
            let mut rejected = Vec::new();
            collect_images(path, &mut scanned, true, 0, &mut rejected);
            for file in scanned {
                if let Some(s) = file.to_str() {
                    entries.push(s.to_string());
                }
            }
        } else {
            entries.push(f.clone());
        }
    }
    entries
}

/// 简单的通配符匹配：支持 *（匹配任意字符）和 ?（匹配单个字符）
fn wildcard_match(name: &str, pattern: &str) -> bool {
    let name_chars: Vec<char> = name.chars().collect();
    let pat_chars: Vec<char> = pattern.chars().collect();
    let mut n = 0;
    let mut p = 0;
    let mut star_n = None;
    let mut star_p = None;

    while n < name_chars.len() {
        if p < pat_chars.len() && (pat_chars[p] == '?' || pat_chars[p] == name_chars[n]) {
            n += 1;
            p += 1;
        } else if p < pat_chars.len() && pat_chars[p] == '*' {
            star_n = Some(n);
            star_p = Some(p);
            p += 1;
        } else if let (Some(sn), Some(sp)) = (star_n, star_p) {
            n = sn + 1;
            star_n = Some(n);
            p = sp + 1;
        } else {
            return false;
        }
    }

    while p < pat_chars.len() && pat_chars[p] == '*' {
        p += 1;
    }

    p == pat_chars.len()
}

/// 用一组模式匹配文件名（任一模式匹配即返回 true）
fn match_patterns(path: &Path, patterns: &[&str]) -> bool {
    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    patterns.iter().any(|p| wildcard_match(file_name, p))
}

fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        matches!(
            ext_lower.as_str(),
            "jpg"
                | "jpeg"
                | "png"
                | "webp"
                | "ico"
                | "tif"
                | "tiff"
                | "dng"
                | "cr2"
                | "cr3"
                | "nef"
                | "arw"
                | "orf"
                | "raf"
                | "rw2"
                | "pef"
                | "srw"
                | "3fr"
        )
    } else {
        false
    }
}

fn save_config(config: &AppConfig) -> Result<()> {
    let config_path = get_config_file_path()?;
    let config_str = toml::to_string_pretty(config)?;
    fs::write(config_path, config_str)?;
    Ok(())
}

fn load_config() -> Result<AppConfig> {
    let config_path = get_config_file_path()?;
    let config_str = fs::read_to_string(config_path)?;
    let config = toml::from_str(&config_str)?;
    Ok(config)
}
