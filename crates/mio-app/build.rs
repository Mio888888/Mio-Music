fn main() {
    println!("cargo:rerun-if-changed=assets");
    // Windows: the exe's own icon (Explorer, shortcuts, the uninstall entry) and, through resource
    // id 1 that gpui loads, the window and taskbar icon.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/icons/icon.ico");
        let icon = icon.to_str().expect("icon path is not UTF-8");
        println!("cargo:rerun-if-changed={icon}");
        let mut res = winresource::WindowsResource::new();
        res.set_icon_with_id(icon, "1")
            .set("ProductName", "Mio Music")
            .set("FileDescription", "Mio Music");
        res.compile().expect("failed to embed the Windows icon");
    }
}
