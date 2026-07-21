// build.rs — 跨平台资源注入
//
// 关键点（2026-07-21 交叉编译验证）：
// build script 是在 HOST（此处为 macOS）上编译运行的，因此
// `#[cfg(target_os = "windows")]` 在这里永远为 false —— 像 `embed-resource`
// 这类靠 cfg(windows) 判断的 crate 在 macOS→Windows 交叉编译时根本不会注入图标。
// 必须用 `CARGO_CFG_TARGET_OS` 这个环境变量（cargo 按真实目标 triple 设置）
// 来判断目标系统，再手写 .rc 调 llvm-rc 编进 exe。

fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    if target_os == "windows" {
        if let Ok(out_dir) = std::env::var("OUT_DIR") {
            let out = std::path::Path::new(&out_dir);

            // 1) 准备图标：优先项目根 icon.ico，复制到 OUT_DIR。
            //    （llvm-rc 对绝对路径的 .rc 文件名有「误判多输入」的坑，
            //     所以一律用相对名 + cd 到 OUT_DIR 解决）
            let src_ico = std::path::Path::new("icon.ico");
            if src_ico.exists() {
                let _ = std::fs::copy(src_ico, out.join("icon.ico"));
            }

            // 2) DPI 感知清单（单引号属性值，避免 raw string 转义坑）
            let manifest = r#"<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<assembly xmlns='urn:schemas-microsoft-com:asm.v1' manifestVersion='1.0'>
  <compatibility xmlns='urn:schemas-microsoft-com:compatibility.v1'>
    <application>
      <supportedOS Id='{e2011457-1546-43c5-a5fe-008deee3d3f0}'/>
      <supportedOS Id='{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}'/>
    </application>
  </compatibility>
  <application xmlns='urn:schemas-microsoft-com:asm.v3'>
    <windowsSettings>
      <dpiAware xmlns='http://schemas.microsoft.com/SMI/2005/WindowsSettings'>true/pm</dpiAware>
      <dpiAwareness xmlns='http://schemas.microsoft.com/SMI/2016/WindowsSettings'>PerMonitorV2, PerMonitor</dpiAwareness>
    </windowsSettings>
  </application>
</assembly>"#;
            let _ = std::fs::write(out.join("app.manifest"), manifest);

            // 3) 写 .rc（图标 + 清单；资源 id 1 为系统约定）
            let rc = "1 ICON \"icon.ico\"\n1 RT_MANIFEST \"app.manifest\"\n";
            let _ = std::fs::write(out.join("app.rc"), rc);

            // 4) 调 llvm-rc：先 cd 到 OUT_DIR，再用相对名 app.rc
            //    （llvm-rc 不接受 -o，输出同名 app.res）
            let rc_bin = std::env::var("RC").unwrap_or_else(|_| "llvm-rc".to_string());
            let status = std::process::Command::new(&rc_bin)
                .current_dir(out)
                .arg("app.rc")
                .status();

            match status {
                Ok(s) if s.success() => {
                    println!("cargo:rustc-link-arg={}/app.res", out_dir);
                }
                _ => {
                    // 失败仅警告，不阻断编译（无图标也能跑）
                    println!(
                        "cargo:warning=llvm-rc 资源注入失败，exe 将无图标/清单（仍可用）"
                    );
                }
            }
        }
    }

    // 变更触发：build.rs 与 icon.ico 改动时重跑
    println!("cargo:rerun-if-changed=build.rs");
    if std::path::Path::new("icon.ico").exists() {
        println!("cargo:rerun-if-changed=icon.ico");
    }
}
