fn main() {
    println!("cargo:rerun-if-changed=icons/icon.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        winresource::WindowsResource::new()
            .set_icon("icons/icon.ico")
            .set("ProductName", "ArchiveFlow")
            .set("FileDescription", "ArchiveFlow native archive workspace")
            .compile()
            .expect("无法编译 Windows 应用图标资源");
    }
}
