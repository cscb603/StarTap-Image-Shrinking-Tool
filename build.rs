fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        // Compile resources.rc which includes the icon and version info
        embed_resource::compile("resources.rc", embed_resource::NONE);
    }
    println!("cargo:rerun-if-changed=resources.rc");
    println!("cargo:rerun-if-changed=icon.ico");
}
