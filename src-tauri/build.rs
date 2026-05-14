fn main() {
    tauri_build::build();

    // On macOS, embed Info.plist into the binary so NSMicrophoneUsageDescription
    // is available even when running outside of a .app bundle (e.g. `tauri dev`).
    // Use CARGO_CFG_TARGET_OS (runtime check) instead of cfg(target_os) (host check)
    // so macOS-specific linker args are not passed when cross-compiling for Android.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "macos" {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let info_plist = format!("{}/Info.plist", manifest_dir);
        println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{}", info_plist);
    }
}
