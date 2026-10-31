fn main() {
    if let Err(error) = tauri_build::try_build(tauri_build::Attributes::new()) {
        println!("cargo::error={error}");
        std::process::exit(1);
    }
}
