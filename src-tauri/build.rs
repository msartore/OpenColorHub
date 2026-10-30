fn main() {
    // Windows only: embed a manifest that requests administrator rights, so the
    // OS raises a UAC prompt at launch instead of silently running asInvoker.
    #[cfg(windows)]
    {
        let attrs = tauri_build::Attributes::new().windows_attributes(
            tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest")),
        );
        tauri_build::try_build(attrs).expect("failed to run tauri-build");
    }

    #[cfg(not(windows))]
    tauri_build::build();
}
