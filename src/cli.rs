use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use rust_image_compressor::{AppConfig, ColorSpace, OutputFormat, ProcessMode};

#[derive(Parser, Debug)]
#[command(name = "rust_image_compressor")]
#[command(about = "图片高速压缩工具 - 高性能 Rust 处理内核", long_about = None)]
pub struct Cli {
    #[arg(long, short = 'i', value_name = "FILE/DIR")]
    pub input: Vec<PathBuf>,

    #[arg(long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    #[arg(long, value_enum, default_value = "custom")]
    pub mode: CliProcessMode,

    #[arg(long, default_value_t = 3000)]
    pub max_dim: u32,

    #[arg(long, default_value_t = 95)]
    pub quality: u8,

    #[arg(long, default_value_t = 0)]
    pub target_kb: u32,

    #[arg(long)]
    pub overwrite: bool,

    #[arg(long)]
    pub keep_original_name: bool,

    #[arg(long, value_enum, default_value = "jpeg")]
    pub output_format: CliOutputFormat,

    #[arg(long)]
    pub json: bool,

    #[arg(long, short = 'q')]
    pub quiet: bool,

    #[arg(value_name = "FILE/DIR")]
    pub positional: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliProcessMode {
    WeChat,
    HD,
    Custom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliOutputFormat {
    Jpeg,
    KeepOriginal,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JsonInput {
    pub version: String,
    pub mode: Option<String>,
    pub quality: Option<u8>,
    pub max_dim: Option<u32>,
    pub target_kb: Option<u32>,
    pub overwrite: Option<bool>,
    pub keep_original_name: Option<bool>,
    pub output_format: Option<String>,
    pub output_dir: Option<String>,
    pub files: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JsonOutput {
    pub success: bool,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub results: Vec<FileResult>,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct FileResult {
    pub input: String,
    pub output: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub original_size: Option<u64>,
    pub compressed_size: Option<u64>,
    pub compression_ratio: Option<f64>,
}

impl From<CliProcessMode> for ProcessMode {
    fn from(mode: CliProcessMode) -> Self {
        match mode {
            CliProcessMode::WeChat => ProcessMode::WeChat,
            CliProcessMode::HD => ProcessMode::HD,
            CliProcessMode::Custom => ProcessMode::Custom,
        }
    }
}

impl From<CliOutputFormat> for OutputFormat {
    fn from(format: CliOutputFormat) -> Self {
        match format {
            CliOutputFormat::Jpeg => OutputFormat::Jpeg,
            CliOutputFormat::KeepOriginal => OutputFormat::KeepOriginal,
        }
    }
}

impl Cli {
    #[allow(dead_code)]
    pub fn to_app_config(&self) -> AppConfig {
        AppConfig {
            mode: self.mode.into(),
            custom_max_dim: self.max_dim,
            custom_quality: self.quality,
            custom_target_kb: self.target_kb,
            overwrite: self.overwrite,
            keep_original_name: self.keep_original_name,
            output_format: self.output_format.into(),
            color_space: ColorSpace::KeepOriginal,
        }
    }
}
