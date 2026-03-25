fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        // Only attempt to compile if resources.rc exists
        if std::path::Path::new("resources.rc").exists() {
            embed_resource::compile("resources.rc", embed_resource::NONE);
        }
    }
    if std::path::Path::new("resources.rc").exists() {
        println!("cargo:rerun-if-changed=resources.rc");
    }
    if std::path::Path::new("icon.ico").exists() {
        println!("cargo:rerun-if-changed=icon.ico");
    }
}
