fn main() {
    tauri_build::build();

    // On macOS, embed Info.plist into the binary so NSMicrophoneUsageDescription
    // is available even when running outside of a .app bundle (e.g. `tauri dev`).
    #[cfg(target_os = "macos")]
    {
        let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let info_plist = format!("{}/Info.plist", manifest_dir);
        println!("cargo:rustc-link-arg=-Wl,-sectcreate,__TEXT,__info_plist,{}", info_plist);
    }
}
