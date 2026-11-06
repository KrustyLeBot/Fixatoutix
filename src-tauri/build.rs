fn main() {
    // Per-Monitor V2 DPI awareness + long paths + Win10/11 compatibility.
    let windows = tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("failed to run tauri-build");
    println!("cargo:rerun-if-env-changed=FIXATOUTIX_BUILD");
}
