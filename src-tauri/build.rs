fn main() {
    tauri_build::build();

    // `tauri dev` starts a plain executable instead of a macOS `.app` bundle.
    // Embed the privacy descriptions so TCC can safely show its permission dialog
    // when the application is launched from RustRover during development.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let info_plist = std::path::PathBuf::from(
            std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by Cargo"),
        )
        .join("Info.plist");
        println!("cargo:rerun-if-changed={}", info_plist.display());
        println!(
            "cargo:rustc-link-arg-bin=local-ai-assistant=-Wl,-sectcreate,__TEXT,__info_plist,{}",
            info_plist.display()
        );
    }
}
