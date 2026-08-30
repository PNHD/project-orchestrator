fn main() {
    // Tauri's Windows resource step requires an ICO. Keep the source asset
    // deterministic and tiny; the product mark remains defined in icon.svg.
    let path = std::path::Path::new("icons/icon.ico");
    if !path.exists() {
        std::fs::create_dir_all("icons").expect("create icon directory");
        let mut ico = vec![
            0, 0, 1, 0, 1, 0, 1, 1, 0, 0, 1, 0, 32, 0, 48, 0, 0, 0, 22, 0, 0, 0,
        ];
        ico.extend_from_slice(&[
            40, 0, 0, 0, 1, 0, 0, 0, 2, 0, 0, 0, 1, 0, 32, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]);
        ico.extend_from_slice(&[15, 45, 32, 255, 0, 0, 0, 0]);
        std::fs::write(path, ico).expect("write icon");
    }
    tauri_build::build()
}
