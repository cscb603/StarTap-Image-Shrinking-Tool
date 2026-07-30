#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 入口路由（三层解耦）：
//! - src/lib.rs    压缩内核（EXIF 保留 / 智能锐化 / 色彩空间 / 感知压缩 / 原子写）
//! - src/gui.rs    人类 GUI（4.1.0 版面 + 三用途卡片，可独立更新）
//! - src/runner.rs CLI / AI-JSON 执行层（信封输出 / 分桶 OOM 护栏 / 配置持久化）
//! - src/cli.rs    参数定义 + capabilities schema + 平台预设表
//!
//! 无参数 → GUI；有参数 → CLI/JSON（--capabilities / --self-check / --json-in 优先）。

mod cli;
mod gui;
mod runner;

use anyhow::Result;
use clap::Parser;

use cli::{build_capabilities, Cli, JsonInput};

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

fn main() -> Result<()> {
    // Windows 终端 UTF-8 编码修正（首行执行，避免中文 log 乱码）
    fix_windows_console();

    // 用 args_os 判断,避免 Windows 上含非 UTF-8 路径时 panic
    if std::env::args_os().count() > 1 {
        let cli = Cli::parse();

        // 并行并发数：AI 可经 --max-workers 限流（在全局池首次使用前生效）
        runner::apply_max_workers(cli.max_workers);

        // --capabilities 优先：输出版本支持的全部参数 schema
        if cli.capabilities {
            let caps = build_capabilities();
            println!("{}", serde_json::to_string_pretty(&caps)?);
            return Ok(());
        }

        // --self-check：环境自检，内置测试图走完整管线，输出健康报告后退出
        if cli.self_check {
            return runner::run_self_check();
        }

        // --json-in 优先：直接传 JSON 字符串，AI 最稳的用法
        if let Some(json_str) = &cli.json_in {
            let json_input: JsonInput = serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("--json-in 解析失败: {}", e))?;
            return runner::run_json_mode(&json_input);
        }

        return runner::run_cli(&cli);
    }

    // 无参数 → 人类 GUI
    gui::run_gui()
}
