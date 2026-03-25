#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cli;

use anyhow::Result;
use clap::Parser;
use crossbeam_channel::{unbounded, Receiver, Sender};
use eframe::egui;
use egui::IconData;
use rayon::prelude::*;
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

use rust_image_compressor::{app_config_to_process_config, AppConfig, OutputFormat, ProcessMode, Processor};
use cli::{Cli, FileResult, JsonInput, JsonOutput};

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
    ProcessingProgress(usize, usize),
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

        let config = load_config().unwrap_or_default();

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
                fonts
                    .font_data
                    .insert("custom_font".to_owned(), egui::FontData::from_owned(data));
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

        let mut visuals = egui::Visuals::light();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.widgets.noninteractive.fg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 41, 59));
        visuals.widgets.noninteractive.rounding = 8.0.into();

        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(255, 255, 255);
        visuals.widgets.inactive.rounding = 8.0.into();

        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(239, 246, 255);
        visuals.widgets.hovered.rounding = 8.0.into();

        visuals.widgets.active.bg_fill = egui::Color32::from_rgb(219, 234, 254);
        visuals.widgets.active.rounding = 8.0.into();

        visuals.selection.bg_fill = egui::Color32::from_rgb(37, 99, 235);
        visuals.window_fill = egui::Color32::from_rgb(248, 250, 252);
        visuals.window_rounding = 12.0.into();

        cc.egui_ctx.set_visuals(visuals);

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
            about_version: "v4.0.0".to_string(),
            tx,
            rx,
        }
    }

    fn add_files(&mut self, paths: Vec<PathBuf>) {
        for path in paths {
            if path.is_dir() {
                if let Ok(entries) = fs::read_dir(&path) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if entry_path.is_file() && is_supported_image(&entry_path) {
                            self.files.push_back(FileItem {
                                path: entry_path,
                                processed: false,
                                success: false,
                                error: None,
                            });
                        }
                    }
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
            let _total = files.len();
            let mut processed = 0;
            let mut success = 0;

            let results: Vec<(usize, bool, Option<String>)> = files
                .par_iter()
                .enumerate()
                .map(|(i, item)| {
                    let result = processor.process_image(&item.path);
                    match result {
                        Ok(_) => (i, true, None),
                        Err(e) => (i, false, Some(e.to_string())),
                    }
                })
                .collect();

            for (i, is_success, _error) in results {
                processed += 1;
                if is_success {
                    success += 1;
                }
                let _ = tx.send(AppEvent::ProcessingProgress(i, if is_success { 1 } else { 0 }));
            }

            let _ = tx.send(AppEvent::ProcessingFinished(processed, success));
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
                let paths: Vec<PathBuf> = files_dropped
                    .into_iter()
                    .filter_map(|f| f.path)
                    .collect();
                self.add_files(paths);
                self.start_processing();
            }
        }

        egui::TopBottomPanel::top("header_panel")
            .frame(
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(20.0, 15.0))
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
            .frame(
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(20.0, 15.0))
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
                                egui::RichText::new(format!("正在处理 {} 个文件...", self.processed_count))
                                    .size(13.0)
                                    .strong()
                                    .color(egui::Color32::from_rgb(37, 99, 235)),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{:.0}%", (self.processed_count as f32 / self.files.len() as f32 * 100.0)))
                                            .size(13.0)
                                            .strong()
                                            .color(egui::Color32::from_rgb(30, 41, 59)),
                                    );
                                },
                            );
                        });
                        ui.add_space(6.0);
                        let pb = egui::ProgressBar::new(self.processed_count as f32 / self.files.len() as f32)
                            .animate(true)
                            .rounding(4.0)
                            .fill(egui::Color32::from_rgb(37, 99, 235));
                        ui.add(pb);
                    } else {
                        ui.label(
                            egui::RichText::new(format!("✨ 准备就绪，待处理 {} 个文件 | 成功 {} 个", self.files.len(), self.success_count))
                                .size(14.0)
                                .strong()
                                .color(egui::Color32::from_rgb(71, 85, 105)),
                        );
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
                egui::Frame::none()
                    .inner_margin(egui::Margin::symmetric(20.0, 10.0))
                    .fill(egui::Color32::from_rgb(248, 250, 252)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(5.0);
                    egui::Frame::none()
                        .fill(egui::Color32::WHITE)
                        .rounding(12.0)
                        .stroke(egui::Stroke::new(
                            1.0,
                            egui::Color32::from_rgb(226, 232, 240),
                        ))
                        .inner_margin(egui::Margin::same(15.0))
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
                                    ui.radio_value(&mut self.config.mode, ProcessMode::WeChat, "微信优化 (900KB)");
                                    ui.add_space(15.0);
                                    ui.radio_value(&mut self.config.mode, ProcessMode::HD, "高清无损 (5MB)");
                                    ui.add_space(15.0);
                                    ui.radio_value(&mut self.config.mode, ProcessMode::Custom, "自定义模式");
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
                        egui::Frame::none()
                            .fill(egui::Color32::WHITE)
                            .rounding(12.0)
                            .stroke(egui::Stroke::new(
                                1.0,
                                egui::Color32::from_rgb(226, 232, 240),
                            ))
                            .inner_margin(egui::Margin::same(15.0))
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
                                                egui::DragValue::new(&mut self.config.custom_max_dim)
                                                    .clamp_range(100..=10000)
                                                    .speed(10.0)
                                                    .suffix(" px"),
                                            );
                                            ui.end_row();

                                            ui.label(
                                                egui::RichText::new("压缩质量 (1-100):")
                                                    .color(egui::Color32::from_rgb(71, 85, 105)),
                                            );
                                            ui.add(egui::Slider::new(&mut self.config.custom_quality, 1..=100));
                                            ui.end_row();

                                            ui.label(
                                                egui::RichText::new("目标大小 (KB):")
                                                    .color(egui::Color32::from_rgb(71, 85, 105)),
                                            );
                                            ui.horizontal(|ui| {
                                                ui.add(
                                                    egui::DragValue::new(&mut self.config.custom_target_kb)
                                                        .clamp_range(0..=50000)
                                                        .speed(10.0)
                                                        .suffix(" KB"),
                                                );
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
                                        );
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
                                                    if let Some(path) = rfd::FileDialog::new().pick_folder() {
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
                                        );
                                        ui.radio_value(&mut self.config.output_format, OutputFormat::Jpeg, "JPG (默认)");
                                        ui.radio_value(&mut self.config.output_format, OutputFormat::KeepOriginal, "保持原始 (仅 PNG)");
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

                    let is_hovering = (ctx.input(|i| !i.raw.hovered_files.is_empty()) || response.hovered()) && !self.processing;

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
                        16.0,
                        bg_color,
                        egui::Stroke::new(stroke_width, stroke_color),
                    );

                    ui.allocate_ui_at_rect(rect, |ui| {
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
                                        .rounding(6.0),
                                    )
                                    .clicked()
                                 {
                                     if let Some(paths) = rfd::FileDialog::new()
                                         .add_filter(
                                             "图片文件",
                                             &["jpg", "jpeg", "png", "webp", "bmp", "dng", "cr2", "cr3", "nef", "arw", "orf", "raf", "rw2", "pef", "srw"],
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

                    if !self.files.is_empty() && !self.processing && ui.button("🚀 开始压缩").clicked() {
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
                        ui.label(egui::RichText::new("📸 图片高速压缩工具").size(20.0).strong());
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
    let args: Vec<String> = std::env::args().collect();

    if args.len() > 1 {
        let cli = Cli::parse();
        return run_cli(&cli);
    }

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
        Box::new(|cc| Box::new(ImageCompressorApp::new(cc))),
    ).map_err(|e| anyhow::anyhow!("GUI error: {}", e))?;

    Ok(())
}

fn load_icon() -> Option<IconData> {
    match ::image::load_from_memory(include_bytes!("../icon.jpg")) {
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
    if cli.json {
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let json_input: JsonInput = serde_json::from_str(&input)?;
        return run_json_mode(&json_input);
    }

    let app_config = cli.to_app_config();
    let process_config = app_config_to_process_config(&app_config, cli.output_dir.clone());
    let processor = Processor::new(process_config);

    let mut files = Vec::new();
    
    // 处理显式的--input参数
    for input_path in &cli.input {
        if input_path.is_dir() {
            if let Ok(entries) = fs::read_dir(input_path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file() && is_supported_image(&entry_path) {
                        files.push(entry_path);
                    }
                }
            }
        } else if input_path.is_file() && is_supported_image(input_path) {
            files.push(input_path.clone());
        }
    }
    
    // 处理SendTo传递的位置参数
    for input_path in &cli.positional {
        if input_path.is_dir() {
            if let Ok(entries) = fs::read_dir(input_path) {
                for entry in entries.flatten() {
                    let entry_path = entry.path();
                    if entry_path.is_file() && is_supported_image(&entry_path) {
                        files.push(entry_path);
                    }
                }
            }
        } else if input_path.is_file() && is_supported_image(input_path) {
            files.push(input_path.clone());
        }
    }

    let _total = files.len();
    let mut completed = 0;
    let mut failed = 0;

    for file in &files {
        if !cli.quiet {
            println!("Processing: {}", file.display());
        }
        match processor.process_image(file) {
            Ok(output_path) => {
                completed += 1;
                if !cli.quiet {
                    println!("  ✅ Success: {}", output_path.display());
                }
            }
            Err(e) => {
                failed += 1;
                if !cli.quiet {
                    println!("  ❌ Failed: {}", e);
                }
            }
        }
    }

    if !cli.quiet {
        println!("\n✅ 处理完成！成功: {}, 失败: {}", completed, failed);
    }

    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn run_json_mode(json_input: &JsonInput) -> Result<()> {
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

    let output_dir = json_input.output_dir.as_ref().map(PathBuf::from);
    let process_config = app_config_to_process_config(&app_config, output_dir);
    let processor = Processor::new(process_config);

    let mut results = Vec::new();
    for file_str in &json_input.files {
        let path = Path::new(file_str);
        let original_size = fs::metadata(path).ok().map(|m| m.len());

        let (success, output, error) = match processor.process_image(path) {
            Ok(output_path) => (true, Some(output_path.display().to_string()), None),
            Err(e) => (false, None, Some(e.to_string())),
        };

        let compressed_size = output.as_ref().and_then(|p| fs::metadata(Path::new(p)).ok().map(|m| m.len()));
        let compression_ratio = if let (Some(orig), Some(comp)) = (original_size, compressed_size) {
            Some(orig as f64 / comp as f64)
        } else {
            None
        };

        results.push(FileResult {
            input: file_str.clone(),
            output,
            success,
            error,
            original_size,
            compressed_size,
            compression_ratio,
        });
    }

    let total = results.len();
    let completed = results.iter().filter(|r| r.success).count();
    let failed = results.iter().filter(|r| !r.success).count();

    let json_output = JsonOutput {
        success: failed == 0,
        total,
        completed,
        failed,
        results,
    };

    println!("{}", serde_json::to_string_pretty(&json_output)?);

    Ok(())
}

fn is_supported_image(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let ext_lower = ext.to_lowercase();
        matches!(ext_lower.as_str(), 
            "jpg" | "jpeg" | "png" | "webp" | "ico" |
            "dng" | "cr2" | "cr3" | "nef" | "arw" | "orf" | "raf" | "rw2" | "pef" | "srw" | "3fr"
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
