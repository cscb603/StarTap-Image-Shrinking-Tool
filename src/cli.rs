use clap::Parser;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use xtap_compress::{AppConfig, ColorSpace, OutputFormat, ProcessMode};

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

    /// 感知量化表：csf=自算CSF感知表(默认) / msssim=内置MS-SSIM调优表 / standard=v4.1.0标准表
    #[arg(long, value_enum, default_value = "csf")]
    pub quant_mode: CliQuantMode,

    /// 画质模式：perceptual=小而美感知压缩(同体积画质更好) / normal=普通标准压缩。
    /// 未指定时跟随 --perceptual 旗标（不给任何新参数 = v4.1.0 旧行为完全不变）
    #[arg(long, value_enum)]
    pub quality_mode: Option<CliQualityMode>,

    /// 用途预设：social=社交分享(按平台预设卡体积线) / archive=高清存档(不缩放+最高画质) / custom=自定义。
    /// 未指定时：给了 --platform 视为 social，否则 custom（旧参数语义不变）
    #[arg(long, value_enum)]
    pub usage_mode: Option<CliUsageMode>,

    /// 平台阈值预设（§2 实测表）：选后自动填长边/体积/Q 并强制 sRGB，规避平台二压
    /// wechat=保守1080/900KB | wechat-new=iOS新宽幅2560/2000KB | xiaohongshu=1440/800KB | instagram=1080/1000KB
    #[arg(long, value_enum)]
    pub platform: Option<CliPlatform>,

    /// 色彩子采样（v4.3.0）：420=照片(默认,省~1/3码率) / 444=截图文字(防文字模糊) / 422=平衡
    #[arg(long)]
    pub subsampling: Option<String>,

    /// 不支持的格式（如 SVG）原样透传复制到输出目录（不压缩、不报失败）。
    /// 配合 --output-dir 使用，便于调用方无需预处理即可 1:1 搬运文件。
    #[arg(long)]
    pub passthrough_unsupported: bool,

    /// 自定义输出文件名后缀（覆盖默认 _wx/_hd/_da）；空串表示无后缀。
    /// 与 --keep-original-name 同时使用时本项无效（后者优先级更高）。
    #[arg(long)]
    pub output_suffix: Option<String>,

    /// v4.4.0：防二压画质优先模式简写（= --quality-mode max）。
    /// 卡平台甜点把画质顶满：Q96 起步 + 4:4:4 色度全保留 + CAS 锐化补偿，体积只作安全线
    #[arg(long)]
    pub quality_first: bool,

    /// v4.4.0：CAS 锐化补偿强度 0.0-1.0（内容自适应、无光晕，专为降采样补锐设计）。
    /// 画质优先档默认 0.35；其他档默认 0（关闭）。仅缩放比 >1.3 时生效
    #[arg(long, value_name = "0.0-1.0")]
    pub cas_strength: Option<f32>,

    /// 覆盖平台默认体积安全线（KB）：触发质量二分搜索压到线内，防止微信/小红书/IG 二次重压
    #[arg(long)]
    pub target_budget_kb: Option<u32>,

    /// 感知模式质量上限（默认95，防止过度堆质量爆体积）
    #[arg(long)]
    pub quality_ceil: Option<u8>,

    /// A/B 对照模式：同一图分别跑旧路径(v4.1.0)与新感知路径，输出 old/new 对照图 + 并排 montage 到 ab_output/
    #[arg(long)]
    pub ab: bool,

    /// 基准对比模式：输出 体积/SSIM/PSNR/各步耗时 对比表（旧路径 vs 新感知路径）
    #[arg(long)]
    pub benchmark: bool,

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
    WebP,
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

impl From<CliFocusMode> for xtap_compress::perceptual::FocusMode {
    fn from(m: CliFocusMode) -> Self {
        match m {
            CliFocusMode::Auto => Self::Auto,
            CliFocusMode::Center => Self::Center,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliQuantMode {
    Standard,
    Msssim,
    Csf,
}

impl From<CliQuantMode> for xtap_compress::perceptual::QuantMode {
    fn from(m: CliQuantMode) -> Self {
        match m {
            CliQuantMode::Standard => Self::Standard,
            CliQuantMode::Msssim => Self::MsSsim,
            CliQuantMode::Csf => Self::Csf,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliPlatform {
    Wechat,
    WechatNew,
    Xiaohongshu,
    Instagram,
    General,
}

impl CliPlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            CliPlatform::Wechat => "wechat",
            CliPlatform::WechatNew => "wechat-new",
            CliPlatform::Xiaohongshu => "xiaohongshu",
            CliPlatform::Instagram => "instagram",
            CliPlatform::General => "general",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliQualityMode {
    /// 小而美：感知压缩（CSF 量化表），同体积画质更好
    Perceptual,
    /// 普通：v4.1.0 标准压缩
    Normal,
    /// v4.4.0 防二压画质优先：Q96 起步 + 4:4:4 + CAS 锐化补偿，卡平台甜点画质顶满
    Max,
}

impl CliQualityMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CliQualityMode::Perceptual => "perceptual",
            CliQualityMode::Normal => "normal",
            CliQualityMode::Max => "max",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum CliUsageMode {
    /// 社交分享：平台预设（长边/体积线/Q/sRGB）防二压
    Social,
    /// 高清存档：不缩放 + 最高画质 + 不限体积
    Archive,
    /// 自定义：完全按 --mode/--max-dim/--quality/--target-kb
    Custom,
}

impl CliUsageMode {
    pub fn as_str(self) -> &'static str {
        match self {
            CliUsageMode::Social => "social",
            CliUsageMode::Archive => "archive",
            CliUsageMode::Custom => "custom",
        }
    }
}

/// 平台阈值预设（蓝图 §2 实测表，2025–2026）
/// 返回：(长边 px 上限, 本地 JPEG Q, 体积安全线 KB, 强制 sRGB)
/// 本地 Q 必须 ≥80（平台二压吞 5–10 点，本地留余地）；强制 sRGB 防社交 CDN 扁平化。
pub fn platform_preset(platform: &str) -> Option<(u32, u8, u32, bool)> {
    match platform.to_lowercase().as_str() {
        // 保守：短边≤1080 用长边上限 1080 直接卡死，兼容所有微信版本
        "wechat" => Some((1080, 92, 900, true)),
        // iOS 8.0.64+ 宽幅：长边≤2560，体积线放宽到 2000KB
        "wechat-new" => Some((2560, 90, 2000, true)),
        // 竖版 3:4：高≤1660（宽1242），体积线 4500KB（官方单图≤5MB）
        "xiaohongshu" => Some((1660, 92, 4500, true)),
        // 4:5：宽≤1080（长边封顶 1080 即保证宽≤1080），体积线建议≤1000KB
        "instagram" => Some((1080, 90, 1000, true)),
        // 通用（中画幅/网盘/非社交渠道）：长边 2560、Q92、2MB 线；不强转 sRGB（保留原色域）
        "general" => Some((2560, 92, 2000, false)),
        _ => None,
    }
}

/// v4.4.0 画质优先预设表（quality_mode="max"，防二压画质优先）
/// 核心思想：卡住平台二压阈值，把画质预算顶到天花板——Q96 起步 + 4:4:4 色度全保留，
/// 体积只是安全线（超线才二分降 Q），**不为压小而压小**。
/// 返回：(长边 px, Q 起步, 体积安全线 KB, 强制 sRGB)
pub fn platform_preset_max(platform: &str) -> Option<(u32, u8, u32, bool)> {
    match platform.to_lowercase().as_str() {
        // 微信保守档（默认）：1080 全版本兼容，900KB 线内 Q96+444 色彩饱满
        "wechat" => Some((1080, 96, 900, true)),
        // iOS 新宽幅：2560 长边，2MB 线
        "wechat-new" => Some((2560, 95, 2000, true)),
        // 小红书：官方单图 ≤5MB，竖版 1242×1660 主流 → 长边 1660 + 4.5MB 安全线
        "xiaohongshu" => Some((1660, 96, 4500, true)),
        "instagram" => Some((1080, 95, 1000, true)),
        // 通用：2560 长边 + 4.9MB 线，不强转 sRGB
        "general" => Some((2560, 96, 4900, false)),
        _ => None,
    }
}

/// v4.4.0 统一收口：按 quality_mode 选预设表并应用到 AppConfig。
/// 三调用点（GUI start_processing / CLI to_app_config / JSON run_json_mode）共用，
/// 防止三处参数表漂移。quality_mode=="max" 时额外强制 4:4:4 + CAS 锐化补偿 0.35
/// （调用方可在本函数返回后用显式参数覆盖）。
pub fn apply_platform_preset(cfg: &mut xtap_compress::AppConfig, platform: &str) {
    let is_max = cfg.quality_mode == "max";
    let preset = if is_max {
        platform_preset_max(platform)
    } else {
        platform_preset(platform)
    };
    if let Some((max_dim, quality, target_kb, srgb)) = preset {
        cfg.custom_max_dim = max_dim;
        cfg.custom_quality = quality;
        cfg.custom_target_kb = target_kb;
        if srgb {
            cfg.color_space = ColorSpace::ConvertToSRGB;
        }
    }
    if is_max {
        cfg.subsampling = "444".to_string();
        cfg.cas_strength = 0.35;
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

    // ========== v4.2.0-exp 感知压缩（AI 用 --json-in，全部 Optional，缺失回退 v4.1.0） ==========
    /// 开启感知压缩模式（降噪+显著性锐化+感知量化表）
    pub perceptual: Option<bool>,
    /// 降噪强度 0-100（仅 perceptual 生效；JPG 输入自动跳过降噪）
    pub denoise_strength: Option<u8>,
    /// 锐化焦点：auto=显著性检测 / center=中心权重
    pub focus_mode: Option<String>,
    /// 覆盖平台默认体积安全线（KB），触发质量二分搜索
    pub target_budget_kb: Option<u32>,
    /// 感知模式质量上限（防止过度堆质量爆体积）
    pub quality_ceil: Option<u8>,
    /// 平台阈值预设：wechat / wechat-new / xiaohongshu / instagram（自动填长边/体积/Q 并强制 sRGB）
    pub platform: Option<String>,
    /// 用途预设：social(社交分享) / archive(高清存档) / custom(自定义)。GUI 用；CLI 可省略
    pub usage_mode: Option<String>,
    /// 画质模式：perceptual(小而美感知压缩) / normal(普通标准压缩) / max(v4.4.0 防二压画质优先)
    pub quality_mode: Option<String>,
    /// 色彩子采样：420=照片(默认) / 444=截图文字 / 422=平衡
    pub subsampling: Option<String>,

    // ========== v4.3.1 工程成熟度提升 ==========
    /// 输出时保留源目录相对路径（默认拍平到 output_dir）
    pub preserve_structure: Option<bool>,
    /// 自定义输出后缀（覆盖默认 _wx/_hd/_da；空串=无后缀）
    pub output_suffix: Option<String>,
    /// 不支持的格式原样透传（不压缩、不报失败），如 SVG
    pub passthrough_unsupported: Option<bool>,

    // ========== v4.4.0 防二压画质优先 ==========
    /// 画质优先简写（= quality_mode:"max"）：Q96 起步 + 4:4:4 + CAS 锐化补偿，卡平台甜点画质顶满
    pub quality_first: Option<bool>,
    /// CAS 锐化补偿强度 0.0-1.0（内容自适应、无光晕；画质优先档默认 0.35，其他档默认 0）
    pub cas_strength: Option<f32>,
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
    /// v4.3.1：跳过（隐藏文件）或透传（不支持但已复制）的数量，不计入 failed
    pub skipped: usize,
    pub results: Vec<FileResult>,
    /// v4.3.1：输入→输出映射清单（含未压缩项），便于 agent 回映射源目录
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub manifest: Vec<ManifestEntry>,
}

/// 输入→输出映射项（v4.3.1），便于调用方回映射源目录
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct ManifestEntry {
    pub input: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    /// compressed | skipped | passthrough | failed
    pub status: String,
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
    /// 错误分类（v4.3.1）：unsupported(格式不支持) / corrupt(解码损坏) / permission(权限) /
    /// skipped(隐藏文件跳过) / passthrough(透传) / error(其他)。便于 agent 决策重试还是跳过。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    /// 透传模式（v4.3.1）：不支持的格式原样复制（success=true）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passthrough: Option<bool>,
    /// v4.2.0-exp 感知压缩指标（perceptual=None 时缺省，向下兼容）；字段缺省不输出
    #[serde(skip_serializing_if = "Option::is_none")]
    pub perceptual: Option<PerceptualMetricsOut>,
}

/// 感知压缩单文件指标（§5.3，输出到 FileResult.perceptual；旧字段缺失时整块缺省）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct PerceptualMetricsOut {
    pub perceptual_mode: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quant_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub denoise_strength: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub focus_mode: Option<String>,
    /// 实际命中的体积安全线（KB）；0 表示未设预算
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes_budget_kb: Option<f64>,
    /// 与源图（降采样后）的 SSIM
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssim_vs_source: Option<f64>,
    /// 与源图（降采样后）的 PSNR(dB)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub psnr_vs_source: Option<f64>,
    /// 实际编码质量（预算二分后）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub final_quality: Option<u8>,
    /// 各步耗时（ms），可观测性
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_timings: Option<StepTimings>,
}

/// 各步耗时（ms）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct StepTimings {
    pub denoise_ms: u64,
    pub downscale_ms: u64,
    pub sharpen_ms: u64,
    pub encode_ms: u64,
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
                "output_format": {"type": "string", "enum": ["jpeg", "original", "webp"], "default": "jpeg", "description": "输出格式（webp 更省体积、支持透明）"},
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
                "max_workers": {"type": "integer", "default": null, "description": "最大并行 worker 数（默认=CPU 核心数），大批量场景可限流"},
                "perceptual": {"type": "boolean", "default": false, "description": "开启感知压缩模式（降噪+显著性锐化+感知量化表），缺省回退 v4.1.0"},
                "denoise_strength": {"type": "integer", "min": 0, "max": 100, "default": 25, "description": "降噪强度，仅 perceptual 生效；JPG 输入自动跳过"},
                "focus_mode": {"type": "string", "enum": ["auto", "center"], "default": "auto", "description": "锐化焦点：auto=显著性检测 / center=中心权重"},
                "target_budget_kb": {"type": "integer", "default": null, "description": "覆盖平台默认体积安全线（KB），触发质量二分搜索"},
                "quality_ceil": {"type": "integer", "min": 1, "max": 100, "default": 95, "description": "感知模式质量上限，防止过度堆质量爆体积"},
                "platform": {"type": "string", "enum": ["wechat", "wechat-new", "xiaohongshu", "instagram"], "default": null, "description": "平台阈值预设，自动填长边/体积/Q 并强制 sRGB"},
                "preserve_structure": {"type": "boolean", "default": false, "description": "输出时保留源目录相对路径（默认拍平到 output_dir）"},
                "output_suffix": {"type": "string", "default": null, "description": "自定义输出后缀（覆盖默认 _wx/_hd/_da；空串=无后缀）"},
                "passthrough_unsupported": {"type": "boolean", "default": false, "description": "不支持的格式（如 SVG）原样透传复制，不报失败"}
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
                description: "输出格式（webp 更省体积、支持透明）".into(),
                available_values: Some(vec!["jpeg".into(), "keep-original".into(), "webp".into()]),
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
                name: "--passthrough-unsupported".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "不支持的格式（如 SVG）原样透传复制到输出目录，不报失败".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--output-suffix".into(),
                short: None,
                kind: "STRING".into(),
                default: "(无，用默认 _wx/_hd/_da)".into(),
                description: "自定义输出文件名后缀（覆盖默认 _wx/_hd/_da；空串=无后缀）".into(),
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
                name: "--platform".into(),
                short: None,
                kind: "STRING".into(),
                default: "(无)".into(),
                description: "平台阈值预设：wechat/wechat-new/xiaohongshu/instagram/general，自动填长边/体积/Q（general 不强转 sRGB，其余强制）".into(),
                available_values: Some(vec!["wechat".into(), "wechat-new".into(), "xiaohongshu".into(), "instagram".into(), "general".into()]),
            },
            CliParamDoc {
                name: "--target-budget-kb".into(),
                short: None,
                kind: "NUMBER".into(),
                default: "(无)".into(),
                description: "覆盖平台默认体积安全线（KB），触发质量二分搜索压到线内".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--quality-ceil".into(),
                short: None,
                kind: "NUMBER".into(),
                default: "95".into(),
                description: "感知模式质量上限，防止过度堆质量爆体积".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--ab".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "A/B 对照：旧路径(v4.1.0)与新感知路径双输出 + 并排 montage 到 ab_output/".into(),
                available_values: None,
            },
            CliParamDoc {
                name: "--benchmark".into(),
                short: None,
                kind: "FLAG".into(),
                default: "false".into(),
                description: "基准对比：输出 体积/SSIM/PSNR/各步耗时 对比表（旧 vs 新）".into(),
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
            CliParamDoc {
                name: "--quality-mode".into(),
                short: None,
                kind: "STRING".into(),
                default: "(未指定时跟随 --perceptual 旗标)".into(),
                description: "画质模式：perceptual=小而美感知压缩(同体积画质更好) / normal=普通标准压缩；显式指定优先于 --perceptual".into(),
                available_values: Some(vec!["perceptual".into(), "normal".into()]),
            },
            CliParamDoc {
                name: "--usage-mode".into(),
                short: None,
                kind: "STRING".into(),
                default: "(未指定时：有 --platform 视为 social，否则 custom)".into(),
                description: "用途预设：social(社交分享,平台预设卡体积线)/archive(高清存档,不缩放最高画质)/custom(自定义)".into(),
                available_values: Some(vec!["social".into(), "archive".into(), "custom".into()]),
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
                "skipped": 0,
                "results": [{"input": "照片.jpg", "output": "照片_da.jpg", "success": true, "error": null, "error_type": null, "original_size": 5000000, "compressed_size": 1200000, "compression_ratio": 4.17}],
                "manifest": [{"input": "照片.jpg", "output": "照片_da.jpg", "status": "compressed"}]
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
            "退出码：0=正常（含隐藏文件跳过/透传）, 1=有真正失败（不支持且未透传/解码损坏/权限）, 2=参数错误".to_string(),
            "stdout 只输出 JSON/JSONL 数据，stderr 放 [INFO]/[WARN]/[ERROR] 分级日志".to_string(),
            "v4.3.1：系统隐藏文件(._*) 归类为 skipped（不计入 failed、退出码仍为 0），不会误触发重试".to_string(),
            "v4.3.1：--output-format webp 输出 WebP（更省体积、支持透明）；透明 PNG→JPEG 自动填白底（修透明丢失）".to_string(),
            "v4.3.1：--preserve-structure 输出时复刻源目录层级；--output-suffix 控制后缀；--passthrough-unsupported 让 SVG 等原样透传".to_string(),
            "v4.3.1：FileResult 新增 error_type(skipped/unsupported/corrupt/permission/passthrough/error)，data 新增 skipped 计数与 manifest 映射清单".to_string(),
            "幂等续跑：输出文件已存在时默认跳过（结果标记 skipped:true），--force 强制重压"
                .to_string(),
            "大批量建议 --jsonl 流式输出：逐行 JSON 可实时采集进度，中断也保留已处理记录"
                .to_string(),
            "首次接入建议先跑 --self-check 验证二进制健康，再跑 --capabilities 获取 schema"
                .to_string(),
            "v4.4.0：quality_mode 新增 max（防二压画质优先）—— Q96 起步 + 4:4:4 全色度保留 + CAS 自然锐化补偿，体积仅作安全线不主动压"
                .to_string(),
            "v4.4.0：--quality-first 简写（= quality_mode max）；--cas-strength 0-1 控制锐化强度（画质优先档默认 0.35，缩放比 >1.3 生效，平坦区自动跳过）"
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
            CliOutputFormat::WebP => OutputFormat::WebP,
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
        let mut cfg = AppConfig {
            config_version: 2,
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
            // 用途推导（三方字段对齐 GUI，向后兼容铁律）：
            // - 显式 --usage-mode → 直接用
            // - 未给 usage-mode 但给了 --platform → social（平台预设驱动）
            // - 都没给 → custom（走 --mode/--max-dim/--quality/--target-kb 旧语义，v4.1.0 完全不变）
            usage_mode: match self.usage_mode {
                Some(u) => u.as_str().to_string(),
                None => {
                    // v4.4.0：--quality-first 也是「平台驱动」信号 → social（默认 wechat 预设）
                    if self.platform.is_some() || self.quality_first {
                        "social".to_string()
                    } else {
                        "custom".to_string()
                    }
                }
            },
            // v4.4.0：--quality-first 简写优先级最高（= max），其次显式 --quality-mode，
            // 都没给则跟随 --perceptual 旗标（v4.1.0 旧行为完全不变）
            quality_mode: if self.quality_first {
                "max".to_string()
            } else {
                self.quality_mode
                    .map(|q| q.as_str().to_string())
                    .unwrap_or_else(|| {
                        if self.perceptual {
                            "perceptual".to_string()
                        } else {
                            "normal".to_string()
                        }
                    })
            },
            platform: self
                .platform
                .map(|p| p.as_str().to_string())
                .unwrap_or_else(|| "wechat".to_string()),
            // v4.3.0：色彩子采样（默认 420 照片；截图文字可切 444 防模糊）
            subsampling: self
                .subsampling
                .clone()
                .unwrap_or_else(|| "420".to_string()),
            // v4.3.1：保结构 / 后缀可控
            preserve_structure: self.preserve_structure,
            output_suffix: self.output_suffix.clone(),
            // v4.4.0：CAS 默认关；画质优先档由 apply_platform_preset 置 0.35，--cas-strength 可覆盖
            cas_strength: 0.0,
        };
        // 平台预设自动填长边/体积/Q 并强制 sRGB（§2）。显式 --target-budget-kb 覆盖预设体积线。
        // --usage-mode social 但没给 --platform 时按默认 wechat 预设（与 GUI 默认一致）。
        let effective_platform = match self.platform {
            Some(p) => Some(p.as_str().to_string()),
            None if cfg.usage_mode == "social" => Some("wechat".to_string()),
            None => None,
        };
        if let Some(ref p) = effective_platform {
            // v4.4.0：统一收口——按 quality_mode 选普通表 / 画质优先表（max：Q96+444+CAS）
            apply_platform_preset(&mut cfg, p);
        }
        // 显式参数优先级最高，预设应用后覆盖（不给则用预设值）
        if let Some(kb) = self.target_budget_kb {
            cfg.custom_target_kb = kb;
        }
        if let Some(ref s) = self.subsampling {
            cfg.subsampling = s.to_lowercase();
        }
        if let Some(cs) = self.cas_strength {
            cfg.cas_strength = cs.clamp(0.0, 1.0);
        }
        cfg
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
    let skipped = results
        .iter()
        .filter(|r| r.skipped.unwrap_or(false) || r.passthrough.unwrap_or(false))
        .count();
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
            skipped,
            results: results.to_vec(),
            manifest: build_manifest(results),
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

/// 由 results 构建输入→输出映射清单（v4.3.1）
fn build_manifest(results: &[FileResult]) -> Vec<ManifestEntry> {
    results
        .iter()
        .map(|r| {
            let status = if !r.success {
                "failed".to_string()
            } else if r.passthrough.unwrap_or(false) {
                "passthrough".to_string()
            } else if r.skipped.unwrap_or(false) {
                "skipped".to_string()
            } else {
                "compressed".to_string()
            };
            ManifestEntry {
                input: r.input.clone(),
                output: r.output.clone(),
                status,
            }
        })
        .collect()
}
