fn main() {
  println!("cargo:rerun-if-changed=icons/icon.ico");
  println!("cargo:rerun-if-changed=icons/icon.png");
  println!("cargo:rerun-if-changed=tauri.conf.json");

  // tauri_build::build() embeds the Windows icon + version resource (from
  // tauri.conf.json `bundle.icon` / `package.version`). A second manual
  // tauri_winres pass here used to ALSO embed a VERSION resource, which the
  // native MSVC resource compiler rejects with `CVT1100: duplicate resource`
  // / `LNK1123` (lld tolerated it during Linux cross-compiles, hiding the bug).
  // Letting tauri_build own the single resource fixes native Windows builds.
  tauri_build::build()
}
