fn main() {
    // На Windows tauri-build вшивает .ico в ресурсы exe, и оттуда же
    // Windows берёт иконку окна. Поэтому debug-сборка (`pnpm tauri dev`)
    // получает цветного кота, а release — контурного.
    let icon = if matches!(std::env::var("PROFILE").as_deref(), Ok("release")) {
        "icons/cat_w.ico"
    } else {
        "icons/cat.ico"
    };

    // cat.ico нет в bundle.icon, поэтому о его изменении cargo надо сказать явно
    println!("cargo:rerun-if-changed=icons/cat.ico");

    let attributes = tauri_build::Attributes::new()
        .windows_attributes(tauri_build::WindowsAttributes::new().window_icon_path(icon));

    tauri_build::try_build(attributes).expect("failed to run tauri-build");
}
