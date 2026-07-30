use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use rust_image_compressor::{AppConfig, ColorSpace, OutputFormat, ProcessMode};

// ============================================================================
// CLI 参数定义
// ============================================================================

#[derive(Parser, Debug)]
#[command(name = "rust_image_compressor")]
#[command(about = "图片高速压缩工具 - 高性能 Rust 处理内核")]
#[command(long_about = "图片高速压缩工具 - 高性能 Rust 处理内核\n\n\
用法示例:\n\
  ./图片高速压缩 -i \"含空格/中文的路径/照片.jpg\"\n\
  ./图片高速压缩 \"目录\"            # 自动递归处理子目录\n\
  ./图片高速压缩 *.jpg             # 通配符由 shell 展开\n\
  echo '{\"files\":[\"路径1\"]}' | ./图片高速压缩 --json\n\
  ./图片高速压缩 --json-in '{\"files\":[\"路径1\"],\"quality\":80}'  # AI 最稳\n\
  ./图片高速压缩 --capabilities     # 输出版本支持的全部参数 schema\n\
  ./图片高速压缩 -i \"照片.jpg\" --dry-run  # 预演，不实际压缩\n\n\
⚠️ 路径含空格或中文:必须加引号包住整个路径\n\
  Windows(cmd): 图片高速压缩.exe \"C:\\用户\\张三\\a b.jpg\"\n\
  macOS/Linux:   ./图片高速压缩 \"~/图片/我的照片.jpg\"\n\
  未加引号会被 shell 按空格拆成多段 → 找不到文件且静默跳过!\n\
  给 AI/脚本最稳妥:用 --json 从 stdin 传路径数组,彻底绕开 shell 分词;\n\
  或 --json-in 直接传 JSON 字符串(零翻译损失)。\n\n\
说明:\n\
  - 目录默认递归(含子目录);--no-recursive 仅处理当前层。\n\
  - RAW(.cr3/.nef/.arw/.dng 等)仅在 macOS 支持(依赖系统 sips);\n\
    Windows 上请先在 Mac 处理,或转成 JPG/PNG。\n\
  - 退出码: 0=正常,1=有失败,2=参数错误。")]
pub struct Cli {
    #[arg(long, short = 'i', value_name = "FILE/DIR")]
    pub input: Vec<PathBuf>,

    #[arg(long, value_name = "DIR")]
    pub output_dir: Option<PathBuf>,

    #[arg(long, value_enum, default_value = "custom")]
    pub mode: CliProcessMode,

    #[arg(long, default_value_t = 3000)]
    pub max_dim: u32,

    #[arg(long, default_value_t = 85)]
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

    /// 直接传 JSON 字符串（AI 最稳，零 shell 分词损失）\n
    /// 如 --json-in '{\"files\":[\"路径1\"],\"quality\":80}'
    #[arg(long, value_name = "JSON")]
    pub json_in: Option<String>,

    #[arg(long, short = 'q')]
    pub quiet: bool,

    /// 目录不递归(默认递归处理子目录)
    #[arg(long)]
    pub no_recursive: bool,

    #[arg(long)]
    pub dry_run: bool,

    /// 输出版本支持的完整参数 schema（AI 能力探测）
    #[arg(long)]
    pub capabilities: bool,

    /// 显式声明是否递归子目录（覆盖默认递归行为）
    #[arg(long)]
    pub recursive: Option<bool>,

    /// Glob 包含模式，如 "*.jpg,*.png"
    #[arg(long, value_name = "GLOB")]
    pub include: Option<String>,

    /// Glob 排除模式，如 "*thumb*"
    #[arg(long, value_name = "GLOB")]
    pub exclude: Option<String>,

    /// 输出时拍平目录结构（不保留源目录相对路径）
    #[arg(long)]
    pub flatten: bool,

    /// 输出时保留源目录相对路径（默认行为）
    #[arg(long)]
    pub preserve_structure: bool,

    // 摄影级优化参数
    #[arg(long)]
    pub enable_sharpening: bool,

    #[arg(long, default_value_t = 1.0)]
    pub sharpening_radius: f32,

    #[arg(long, default_value_t = 0.8)]
    pub sharpening_amount: f32,

    #[arg(long)]
    pub use_custom_quantization: bool,

    #[arg(long)]
    pub preserve_high_frequency: bool,

    #[arg(long, value_enum, default_value = "keep")]
    pub color_space: CliColorSpace,

    /// 强制重压：即使目标输出文件已存在也重新压缩（默认已存在则跳过，幂等续跑）
    #[arg(long)]
    pub force: bool,

    /// 流式 JSONL 输出：每处理完一个文件立即输出一行 JSON，末尾追加汇总信封
    #[arg(long)]
    pub jsonl: bool,

    /// 最大并行 worker 数（默认 = CPU 核心数），大批量场景可限流
    #[arg(long, value_name = "N")]
    pub max_workers: Option<usize>,

    /// 环境自检：内置生成测试图完整走一遍压缩管线，输出健康报告后退出
    #[arg(long)]
    pub self_check: bool,

    // ========== v4.2.0-exp 感知压缩（仅 CLI/AI 开放，GUI 锁死 v4.1.0） ==========
    /// 开启感知压缩模式：降噪+显著性锐化+感知量化表（默认关，旧行为完全不变）
    #[arg(long)]
    pub perceptual: bool,

    /// 降噪强度 0-100（默认 25，仅 --perceptual 生效；JPG 输入自动跳过降噪防块效应）
    #[arg(long, value_name = "0-100", default_value_t = 25)]
    pub denoise_strength: u8,

    /// 锐化焦点：auto=显著性检测（主体自动识别），center=中心权重
    #[arg(long, value_enum, default_value = "auto")]
    pub focus_mode: CliFocusMode,

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliColorSpace {
    Keep,
    SRgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliFocusMode {
    Auto,
    Center,
}

impl From<CliFocusMode> for rust_image_compressor::perceptual::FocusMode {
    fn from(m: CliFocusMode) -> Self {
        match m {
            CliFocusMode::Auto => Self::Auto,
            CliFocusMode::Center => Self::Center,
        }
    }
}

// ============================================================================
// JSON 入参（AI 用 --json-in / --json stdin）
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JsonInput {
    pub version: Option<String>,
    pub mode: Option<String>,
    pub quality: Option<u8>,
    pub max_dim: Option<u32>,
    pub target_kb: Option<u32>,
    pub overwrite: Option<bool>,
    pub keep_original_name: Option<bool>,
    pub output_format: Option<String>,
    pub output_dir: Option<String>,
    pub files: Vec<String>,

    // 摄影级优化
    pub enable_sharpening: Option<bool>,
    pub sharpening_radius: Option<f32>,
    pub sharpening_amount: Option<f32>,
    pub use_custom_quantization: Option<bool>,
    pub preserve_high_frequency: Option<bool>,
    pub color_space: Option<String>,

    // 目录遍历
    pub recursive: Option<bool>,
    pub include_pattern: Option<String>,
    pub exclude_pattern: Option<String>,

    // 输出策略
    pub flatten: Option<bool>,
    pub dry_run: Option<bool>,

    // 工业级调度（v4.1.0）
    /// 强制重压（默认输出已存在则跳过，幂等续跑）
    pub force: Option<bool>,
    /// 流式 JSONL：每个文件一行 JSON + 末尾汇总信封
    pub jsonl: Option<bool>,
    /// 最大并行 worker 数
    pub max_workers: Option<usize>,
}

// ============================================================================
// JSON 输出信封（Agent-First 规范）
// ============================================================================

/// 标准 JSON 信封（Agent-First 规范）：\n\
/// stdout 只放此 JSON，stderr 放日志/警告。\n\
/// schema_version 固定 1.0，command 固定 compress。
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JsonEnvelope {
    pub schema_version: String,
    pub command: String,
    pub status: String,
    pub data: JsonOutputData,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    pub metrics: JsonMetrics,
}

#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JsonOutputData {
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub results: Vec<FileResult>,
}

#[derive(Debug, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct JsonMetrics {
    pub original_bytes: u64,
    pub compressed_bytes: u64,
    pub bytes_saved: u64,
    /// 压缩比 = 原始 / 压缩，>1 说明变小了
    pub avg_ratio: f64,
    pub total_time_ms: u64,
}

/// 旧版 JsonOutput 保留兼容（被 JsonEnvelope.data 替代）\n\
/// 新代码请用 JsonEnvelope
#[derive(Debug, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct JsonOutput {
    pub success: bool,
    pub total: usize,
    pub completed: usize,
    pub failed: usize,
    pub results: Vec<FileResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct FileResult {
    pub input: String,
    pub output: Option<String>,
    pub success: bool,
    pub error: Option<String>,
    pub original_size: Option<u64>,
    pub compressed_size: Option<u64>,
    pub compression_ratio: Option<f64>,
    /// 幂等续跑：输出已存在且未 --force 时跳过（success=true, skipped=true）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<bool>,
}

// ============================================================================
// 能力探测（--capabilities 输出）
// ============================================================================

#[derive(Debug, Serialize)]
pub struct Capabilities {
    pub schema_version: String,
    pub tool_name: String,
    pub version: String,
    /// 运行环境信息：cpu_count / 推荐并行数（AI 可据此设置 max_workers）
    pub runtime: serde_json::Value,
    pub json_input_schema: serde_json::Value,
    pub cli_parameters: Vec<CliParamDoc>,
    pub json_output_envelope: serde_json::Value,
    pub notes: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CliParamDoc {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub short: Option<String>,
    pub kind: String,
    pub default: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub available_values: Option<Vec<String>>,
}

/// 生成 --capabilities 的完整参数 schema
pub fn build_capabilities() -> Capabilities {
    let _json_example = serde_json::json!({
        "files": ["照片1.jpg", "照片2.png"],
        "mode": "custom",
        "quality": 95,
        "max_dim": 3000,
        "target_kb": 0,
        "overwrite": false,
        "keep_original_name": false,
        "output_format": "jpeg",
        "output_dir": "/输出路径",
        "enable_sharpening": false,
        "sharpening_radius": 1.0,
        "sharpening_amount": 0.8,
        "use_custom_quantization": false,
        "preserve_high_frequency": false,
        "color_space": "keep",
        "recursive": true,
        "include_pattern": null,
        "exclude_pattern": null,
        "flatten": false,
        "dry_run": false
    });

    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    Capabilities {
        schema_version: "1.1".to_string(),
        tool_name: env!("CARGO_PKG_NAME").to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        runtime: serde_json::json!({
            "cpu_count": cpu_count,
            "recommended_max_workers": cpu_count,
            "description": "默认并行数=cpu_count；服务器共享负载建议 max_workers 设为 cpu_count/2"
        }),
        json_input_schema: serde_json::json!({
            "description": "--json-in 或 --json stdin 接受的 JSON 结构",
            "required": ["files"],
            "properties": {
                "files": {"type": "array", "items": {"type": "string"}, "description": "待处理的文件路径列表（必填）"},
                "mode": {"type": "string", "enum": ["wechat", "hd", "custom"], "default": "custom", "description": "处理模式"},
                "quality": {"type": "integer", "min": 1, "max": 100, "default": 95, "description": "JPEG 压缩质量"},
                "max_dim": {"type": "integer", "default": 3000, "description": "最长边像素，0=不缩放"},
                "target_kb": {"type": "integer", "default": 0, "description": "目标体积 KB，0=不限"},
                "overwrite": {"type": "boolean", "default": false, "description": "覆盖原文件"},
                "keep_original_name": {"type": "boolean", "default": false, "description": "保留原文件名（不加后缀）"},
                "output_format": {"type": "string", "enum": ["jpeg", "original"], "default": "jpeg", "description": "输出格式"},
                "output_dir": {"type": "string", "default": null, "description": "输出目录，未指定时默认 ./compressed/"},
                "enable_sharpening": {"type": "boolean", "default": false, "description": "启用智能自适应锐化"},
                "sharpening_radius": {"type": "number", "default": 1.0, "description": "锐化半径"},
                "sharpening_amount": {"type": "number", "default": 0.8, "description": "锐化强度"},
                "use_custom_quantization": {"type": "boolean", "default": false, "description": "使用自定义量化表"},
                "preserve_high_frequency": {"type": "boolean", "default": false, "description": "保留高频细节"},
                "color_space": {"type": "string", "enum": ["keep", "srgb"], "default": "keep", "description": "色彩空间处理"},
                "recursive": {"type": "boolean", "default": true, "description": "目录递归处理子目录"},
                "include_pattern": {"type": "string", "default": null, "description": "包含的 Glob 模式，如 *.jpg,*.png"},
                "exclude_pattern": {"type": "string", "default": null, "description": "排除的 Glob 模式，如 *thumb*"},
                "flatten": {"type": "boolean", "default": false, "description": "输出时拍平目录结构"},
                "dry_run": {"type": "boolean", "default": false, "description": "预演模式，不执行压缩"},
                "force": {"type": "boolean", "default": false, "description": "强制重压：即使输出已存在也重新压缩（默认已存在则跳过，幂等续跑）"},
                "jsonl": {"type": "boolean", "default": false, "description": "流式 JSONL：每处理完一个文件立即输出一行 JSON，末尾追加汇总信封"},
                "max_workers": {"type": "integer", "default": null, "description": "最大并行 worker 数（默认=CPU 核心数），大批量场景可限流"}
            }
        }),
        cli_parameters: vec![
            CliParamDoc {
                name: "--input / -i".into(),
                short: Some("-i".into()),
                kind: "FILE/DIR (可多次)".into(),
                default: "(无)".into(),
                description: "指定输入文件或目录".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--output-dir".into(),
                short: None,
                kind: "DIR".into(),
                default: "./compressed/".into(),
                description: "输出目录".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--mode".into(),
                short: None,
                kind: "STRING".into(),
                default: "custom".into(),
                description: "处理模式".into(),
                available_values: Some(vec!["wechat".into(), "hd".into(), "custom".into()]),
            },
            CliParamDoc {
                name: "--max-dim".into(),
                short: None,
                kind: "NUMBER".into(),
                default: "3000".into(),
                description: "最长边像素".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--quality".into(),
                short: None,
                kind: "NUMBER".into(),
                default: "95".into(),
                description: "JPEG 质量 1-100".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--target-kb".into(),
                short: None,
                kind: "NUMBER".into(),
                default: "0".into(),
                description: "目标体积 KB".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--overwrite".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "覆盖原文件".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--keep-original-name".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "保留原文件名".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--output-format".into(),
                short: None,
                kind: "STRING".into(),
                default: "jpeg".into(),
                description: "输出格式".into(),
                available_values: Some(vec!["jpeg".into(), "keep-original".into()]),
            },
            CliParamDoc {
                name: "--json".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "JSON 模式（stdin 输入 / 输出 JSON 信封）".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--json-in".into(),
                short: None,
                kind: "JSON".into(),
                default: "(无)".into(),
                description: "直接传 JSON 字符串（AI 最稳）".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--dry-run".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "预演模式：扫描文件但不压缩".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--capabilities".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "输出版本支持的全部参数 schema".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--recursive".into(),
                short: None,
                kind: "BOOL".into(),
                default: "true".into(),
                description: "目录递归处理子目录".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--no-recursive".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "仅处理当前目录层".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--include".into(),
                short: None,
                kind: "GLOB".into(),
                default: "(无)".into(),
                description: "包含的 Glob 模式，如 *.jpg,*.png".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--exclude".into(),
                short: None,
                kind: "GLOB".into(),
                default: "(无)".into(),
                description: "排除的 Glob 模式，如 *thumb*".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--flatten".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "输出时拍平目录结构".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--preserve-structure".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "输出时保留源目录相对路径".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--enable-sharpening".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "启用智能自适应锐化".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--sharpening-radius".into(),
                short: None,
                kind: "FLOAT".into(),
                default: "1.0".into(),
                description: "锐化半径".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--sharpening-amount".into(),
                short: None,
                kind: "FLOAT".into(),
                default: "0.8".into(),
                description: "锐化强度".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--color-space".into(),
                short: None,
                kind: "STRING".into(),
                default: "keep".into(),
                description: "色彩空间".into(),
                available_values: Some(vec!["keep".into(), "srgb".into()]),
            },
            CliParamDoc {
                name: "--force".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "强制重压：即使输出已存在也重新压缩（默认已存在则跳过，幂等续跑）"
                    .into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--jsonl".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "流式 JSONL 输出：每个文件一行 JSON + 末尾汇总信封".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--max-workers".into(),
                short: None,
                kind: "NUMBER".into(),
                default: "(CPU核心数)".into(),
                description: "最大并行 worker 数，大批量场景可限流".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--self-check".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "环境自检：内置测试图完整走一遍压缩管线，输出健康报告".into(),
                available_values: None,
            },
        ],
        json_output_envelope: serde_json::json!({
            "schema_version": "1.0",
            "command": "compress",
            "status": "succeeded | partial",
            "data": {
                "total": 5,
                "completed": 5,
                "failed": 0,
                "results": [{"input": "照片.jpg", "output": "照片_da.jpg", "success": true, "error": null, "original_size": 5000000, "compressed_size": 1200000, "compression_ratio": 4.17}]
            },
            "warnings": [],
            "errors": [],
            "metrics": {"original_bytes": 5000000, "compressed_bytes": 1200000, "bytes_saved": 3800000, "avg_ratio": 4.17, "total_time_ms": 1234}
        }),
        notes: vec![
            "路径含空格/中文必须用引号包住！AI 最稳：--json-in".to_string(),
            "RAW 格式仅在 macOS 支持；Windows 会明确报错".to_string(),
            "目录默认递归（最深 20 层），--no-recursive 仅当前层".to_string(),
            "未指定 --output-dir 时默认输出到 ./compressed/ 目录".to_string(),
            "退出码：0=正常, 1=有失败, 2=参数错误".to_string(),
            "stdout 只输出 JSON/JSONL 数据，stderr 放 [INFO]/[WARN]/[ERROR] 分级日志".to_string(),
            "幂等续跑：输出文件已存在时默认跳过（结果标记 skipped:true），--force 强制重压"
                .to_string(),
            "大批量建议 --jsonl 流式输出：逐行 JSON 可实时采集进度，中断也保留已处理记录"
                .to_string(),
            "首次接入建议先跑 --self-check 验证二进制健康，再跑 --capabilities 获取 schema"
                .to_string(),
        ],
    }
}

// ============================================================================
// 类型转换
// ============================================================================

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

impl From<CliColorSpace> for ColorSpace {
    fn from(cs: CliColorSpace) -> Self {
        match cs {
            CliColorSpace::Keep => ColorSpace::KeepOriginal,
            CliColorSpace::SRgb => ColorSpace::ConvertToSRGB,
        }
    }
}

impl Cli {
    pub fn to_app_config(&self) -> AppConfig {
        AppConfig {
            mode: self.mode.into(),
            custom_max_dim: self.max_dim,
            custom_quality: self.quality,
            custom_target_kb: self.target_kb,
            overwrite: self.overwrite,
            keep_original_name: self.keep_original_name,
            output_format: self.output_format.into(),
            color_space: self.color_space.into(),
            enable_sharpening: self.enable_sharpening,
            sharpening_radius: self.sharpening_radius,
            sharpening_amount: self.sharpening_amount,
            use_custom_quantization: self.use_custom_quantization,
            preserve_high_frequency: self.preserve_high_frequency,
        }
    }
}

// ============================================================================
// JSON 信封构建
// ============================================================================

/// 从 results 构建标准 JSON 信封
pub fn build_envelope(results: &[FileResult], start: std::time::Instant) -> JsonEnvelope {
    let total = results.len();
    let completed = results.iter().filter(|r| r.success).count();
    let failed = total - completed;
    let original_bytes: u64 = results.iter().filter_map(|r| r.original_size).sum();
    let compressed_bytes: u64 = results.iter().filter_map(|r| r.compressed_size).sum();
    let bytes_saved = original_bytes.saturating_sub(compressed_bytes);
    let avg_ratio = if compressed_bytes > 0 && original_bytes > 0 {
        original_bytes as f64 / compressed_bytes as f64
    } else {
        1.0
    };

    JsonEnvelope {
        schema_version: "1.0".to_string(),
        command: "compress".to_string(),
        status: if failed == 0 {
            "succeeded".to_string()
        } else {
            "partial".to_string()
        },
        data: JsonOutputData {
            total,
            completed,
            failed,
            results: results.to_vec(),
        },
        warnings: vec![],
        errors: vec![],
        metrics: JsonMetrics {
            original_bytes,
            compressed_bytes,
            bytes_saved,
            avg_ratio,
            total_time_ms: start.elapsed().as_millis() as u64,
        },
    }
}
