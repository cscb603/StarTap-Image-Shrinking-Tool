//! CLI / AI-JSON 执行层（人类 GUI 之外的两条通路）
//!
//! 与 GUI 共用 lib.rs 内核（app_config_to_process_config / Processor）。
//! 本层职责：参数→配置映射、文件收集、并行调度（大图串行 OOM 护栏）、JSON 信封输出、配置持久化。
//! v4.2.0 三方契约：usage_mode(social/archive/custom) + quality_mode(perceptual/normal) + platform
//! 与 GUI 语义完全一致；不带新参数时 100% 兼容 v4.1.0 旧行为。

use anyhow::Result;
use image::GenericImageView;
use rayon::prelude::*;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::cli::{
    build_envelope, platform_preset, Cli, CliQualityMode, FileResult, JsonInput,
    PerceptualMetricsOut, StepTimings,
};
use rust_image_compressor::perceptual::{
    FocusMode, PerceptualMetrics, PerceptualOptions, QuantMode,
};
use rust_image_compressor::{
    app_config_to_process_config, AppConfig, ColorSpace, OutputFormat, ProcessMode, Processor,
};

// ============================================================================
// 配置持久化（GUI / CLI 共用）
// ============================================================================

pub(crate) fn get_config_file_path() -> Result<PathBuf> {
    if let Some(mut path) = dirs::config_dir() {
        path.push("rust_image_compressor");
        fs::create_dir_all(&path)?;
        path.push("config.toml");
        Ok(path)
    } else {
        Ok(PathBuf::from("image_compressor_config.toml"))
    }
}

pub(crate) fn save_config(config: &AppConfig) -> Result<()> {
    let config_path = get_config_file_path()?;
    let config_str = toml::to_string_pretty(config)?;
    fs::write(config_path, config_str)?;
    Ok(())
}

pub(crate) fn load_config() -> Result<AppConfig> {
    let config_path = get_config_file_path()?;
    let config_str = fs::read_to_string(config_path)?;
    let config = toml::from_str(&config_str)?;
    Ok(config)
}

// ============================================================================
// 感知压缩开关（CLI 三态契约）
// ============================================================================

/// 从 CLI 参数构造感知压缩选项。
/// 三态契约（向后兼容铁律）：
/// - 显式 `--quality-mode perceptual` → 开
/// - 显式 `--quality-mode normal`     → 关（即使给了 --perceptual 也关，normal 语义优先）
/// - 未指定 quality-mode              → 跟随 `--perceptual` 旗标（v4.1.0 旧行为完全不变）
fn perceptual_options_from_cli(cli: &Cli) -> Option<PerceptualOptions> {
    let on = match cli.quality_mode {
        Some(CliQualityMode::Perceptual) => true,
        Some(CliQualityMode::Normal) => false,
        None => cli.perceptual,
    };
    if !on {
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

// ============================================================================
// 大图串行 / 小图并行 分桶调度（与 GUI 同款 OOM 护栏，CLI/JSON 通路共用）
// ============================================================================

/// 判定是否需要串行处理的大图。
/// 命中任一即视为大图：TIFF 后缀（解码后内存远超文件体积）/
/// 原始文件 > 80MB / 解码后像素 > 4000 万（约 8000×5000）。
/// 大图串行可确保内存峰值 = 单张，避免多张并行解码撑爆内存。
pub(crate) fn is_large_image(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let e = ext.to_ascii_lowercase();
        if e == "tif" || e == "tiff" {
            return true;
        }
    }
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() > 80 * 1024 * 1024 {
            return true;
        }
    }
    if let Ok((w, h)) = image::image_dimensions(path) {
        if (w as u64) * (h as u64) > 40_000_000 {
            return true;
        }
    }
    false
}

/// 分桶映射执行：小图并行（rayon 全局池）、大图串行（内存峰值=单张），
/// 结果严格按输入顺序返回（与 par_iter().map().collect() 输出顺序一致）。
fn map_bucketed<T, R, P, F>(items: &[T], big_pred: P, f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    P: Fn(&T) -> bool,
    F: Fn(&T) -> R + Sync,
{
    use std::sync::Mutex;
    let n = items.len();
    let slots: Vec<Mutex<Option<R>>> = (0..n).map(|_| Mutex::new(None)).collect();
    let mut big_idx: Vec<usize> = Vec::new();
    let mut small_idx: Vec<usize> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        if big_pred(it) {
            big_idx.push(i);
        } else {
            small_idx.push(i);
        }
    }
    // 小图并行
    small_idx.par_iter().for_each(|&i| {
        let r = f(&items[i]);
        *slots[i].lock().unwrap() = Some(r);
    });
    // 大图串行（一次一张，绝不叠加）
    for &i in &big_idx {
        let r = f(&items[i]);
        *slots[i].lock().unwrap() = Some(r);
    }
    slots
        .into_iter()
        .map(|m| m.into_inner().unwrap().expect("map_bucketed: 槽位未填充"))
        .collect()
}

pub(crate) fn run_cli(cli: &Cli) -> Result<()> {
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
        // stdin 挂死修复（P0）：只有「没给任何文件/目录参数」时才读 stdin。
        // 旧逻辑无条件 read_to_string(stdin)，AI 子进程若不关 stdin 会永久阻塞。
        let no_file_args = cli.input.is_empty() && cli.positional.is_empty();
        if no_file_args {
            // 没给文件参数 → JSON 载荷应来自 stdin（管道用法：echo '{...}' | app --json）
            let mut stdin_input = String::new();
            let stdin_result = std::io::stdin().read_to_string(&mut stdin_input);
            if stdin_result.is_ok() && !stdin_input.trim().is_empty() {
                let json_input: JsonInput = serde_json::from_str(&stdin_input)?;
                return run_json_mode(&json_input);
            }
            eprintln!("⚠️ --json 模式：未给文件参数且 stdin 无 JSON 载荷。");
            print_cli_hint();
            std::process::exit(2);
        } else {
            // 给了文件参数 → 直接用 CLI 参数处理并输出 JSON 信封，绝不碰 stdin
            if files.is_empty() {
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

    // 分桶调度（与 GUI 同款 OOM 护栏）：小图并行、大图串行，统一走 process_one_file
    let quiet = cli.quiet;
    let force = cli.force;
    let overwrite = cli.overwrite;
    let jsonl = cli.jsonl;
    let results: Vec<FileResult> = map_bucketed(
        &files,
        |f| is_large_image(f),
        |file| {
            let r = process_one_file(&processor, file, force, overwrite);
            if jsonl {
                emit_jsonl(&r);
            }
            r
        },
    );

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
pub(crate) fn collect_images(
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
    // 分桶调度（与 GUI 同款 OOM 护栏）：小图并行、大图串行
    let results: Vec<FileResult> = map_bucketed(
        files,
        |f| is_large_image(f),
        |file| {
            let result = process_one_file(&processor, file, force, overwrite);
            if jsonl {
                // 流式 JSONL：每处理完一个文件立即输出一行（println! 自带行级锁）
                emit_jsonl(&result);
            }
            result
        },
    );

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
        bytes_budget_kb: if budget > 0 {
            Some(budget as f64)
        } else {
            None
        },
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
pub(crate) fn apply_max_workers(n: Option<usize>) {
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
        .or_else(|| {
            Some(
                std::env::current_dir()
                    .unwrap_or_default()
                    .join("compressed"),
            )
        })
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
            "{:<24} {:>9} {:>9} {:>8} {:>8} {:>8} {:>8} {:>6} {:>7}",
            "file", "old_KB", "new_KB", "oldSSIM", "newSSIM", "oldPSNR", "newPSNR", "ms", "newQ"
        );
    }

    let mut montage_rows: Vec<String> = Vec::new();
    for file in files {
        let t0 = std::time::Instant::now();
        let old_r = process_one_file(&old_proc, file, force, overwrite);
        let new_r = process_one_file(&new_proc, file, force, overwrite);
        let elapsed_ms = t0.elapsed().as_millis();

        let old_size = old_r.compressed_size.unwrap_or(0);
        let new_size = new_r.compressed_size.unwrap_or(0);

        // 旧/新 各自与源图（降采样后）的 SSIM/PSNR，外部核算、算法同源
        let (old_ssim, old_psnr) = old_r
            .output
            .as_ref()
            .and_then(|p| ssim_psnr_vs_source(file, Path::new(p)))
            .unwrap_or((f64::NAN, f64::NAN));
        let (new_ssim, new_psnr) = new_r
            .output
            .as_ref()
            .and_then(|p| ssim_psnr_vs_source(file, Path::new(p)))
            .unwrap_or((f64::NAN, f64::NAN));
        let new_q = new_r
            .perceptual
            .as_ref()
            .and_then(|m| m.final_quality)
            .unwrap_or(0);

        if cli.benchmark {
            println!(
                "{:<24} {:>9.1} {:>9.1} {:>8.4} {:>8.4} {:>8.2} {:>8.2} {:>6} {:>7}",
                file.file_name()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default(),
                old_size as f64 / 1024.0,
                new_size as f64 / 1024.0,
                old_ssim,
                new_ssim,
                old_psnr,
                new_psnr,
                elapsed_ms,
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
    let orig_resized = image::imageops::resize(
        &orig_img.to_rgb8(),
        ow,
        oh,
        image::imageops::FilterType::Triangle,
    );
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
pub(crate) fn run_self_check() -> Result<()> {
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

pub(crate) fn run_json_mode(json_input: &JsonInput) -> Result<()> {
    let start = std::time::Instant::now();

    // 用途推导（三方对齐，向后兼容铁律）：
    // - 显式 usage_mode → 直接用
    // - 未给 usage_mode 但给了 platform → social（平台预设驱动）
    // - 都没给 → custom（走 mode/quality/max_dim 旧字段，v4.1.0 JSON 调用 100% 不变）
    let usage_mode = match json_input.usage_mode.as_deref() {
        Some("social") => "social".to_string(),
        Some("archive") => "archive".to_string(),
        Some("custom") => "custom".to_string(),
        Some(other) => {
            eprintln!("[WARN] 未知 usage_mode '{}'，按 custom 处理", other);
            "custom".to_string()
        }
        None => {
            if json_input.platform.is_some() {
                "social".to_string()
            } else {
                "custom".to_string()
            }
        }
    };
    let mut app_config = AppConfig {
        usage_mode,
        ..Default::default()
    };
    if let Some(plat) = &json_input.platform {
        app_config.platform = plat.clone();
    }

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
    // v4.3.0：色彩子采样（JSON 可覆盖默认 420）
    if let Some(s) = &json_input.subsampling {
        let s = s.to_lowercase();
        if s == "444" || s == "422" || s == "420" {
            app_config.subsampling = s;
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
    // 感知开关三态契约（与 CLI/GUI 对齐）：
    // - 显式 quality_mode="perceptual" → 开；"normal" → 关
    // - 缺失但显式 usage_mode="social" → 开（新式调用，与 GUI 社交分享默认一致）
    // - 都缺失 → 跟随旧字段 perceptual（默认关 → v4.1.0 JSON 调用 100% 兼容）
    let perceptual_on = match json_input.quality_mode.as_deref() {
        Some("perceptual") => true,
        Some("normal") => false,
        Some(other) => {
            eprintln!("[WARN] 未知 quality_mode '{}'，按 normal 处理", other);
            false
        }
        None => {
            if json_input.usage_mode.as_deref() == Some("social") {
                true
            } else {
                json_input.perceptual.unwrap_or(false)
            }
        }
    };
    process_config.perceptual = if perceptual_on {
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
    // 分桶调度（与 GUI 同款 OOM 护栏）：小图并行、大图串行（多张超大 TIFF 不会并行解码撑爆内存）
    let results: Vec<FileResult> = map_bucketed(
        &all_entries,
        |e| is_large_image(Path::new(e)),
        |entry| {
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
        },
    );

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

pub(crate) fn is_supported_image(path: &Path) -> bool {
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
