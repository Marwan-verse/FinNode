use std::path::PathBuf;

fn main() {
  println!("cargo:rerun-if-changed=icons/icon.ico");
  println!("cargo:rerun-if-changed=icons/icon.png");
  println!("cargo:rerun-if-changed=tauri.conf.json");

  let target = std::env::var("TARGET").unwrap_or_default();

  if target.contains("windows") {
    let manifest_dir = PathBuf::from(
      std::env::var("CARGO_MANIFEST_DIR").expect("missing CARGO_MANIFEST_DIR"),
    );
    let icon_path = manifest_dir.join("icons").join("icon.ico");

    if !icon_path.exists() {
      panic!("missing Windows icon file: {}", icon_path.display());
    }

    let mut res = tauri_winres::WindowsResource::new();
    res.set_icon(icon_path.to_string_lossy().as_ref());
    res.compile().expect("failed to compile Windows resources");
  }

  tauri_build::build()
}
