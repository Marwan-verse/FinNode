fn main() {
  let target = std::env::var("TARGET").unwrap_or_default();

  if target.contains("windows") {
    let mut res = tauri_winres::WindowsResource::new();
    res.set_icon("icons/icon.ico");
    res.compile().expect("failed to compile Windows resources");
  }

  tauri_build::build()
}
