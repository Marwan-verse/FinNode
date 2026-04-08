#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use notify::{RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
  fs,
  fs::{File, OpenOptions},
  io,
  path::{Path, PathBuf},
  process::Command,
  sync::{Arc, Mutex, OnceLock},
  thread,
};
use fs2::FileExt;
use tauri::{
  AppHandle, CustomMenuItem, GlobalShortcutManager, Manager, State, SystemTray, SystemTrayEvent,
  SystemTrayMenu, SystemTrayMenuItem, Window, WindowEvent,
};

static APP_INSTANCE_LOCK: OnceLock<File> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LaunchTargets {
  path: Option<String>,
  editor: Option<String>,
  browser: Option<String>,
  script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProjectNode {
  id: String,
  name: String,
  icon: String,
  description: String,
  x: f64,
  y: f64,
  links: Vec<String>,
  targets: LaunchTargets,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct ProjectLayout {
  nodes: Vec<ProjectNode>,
}

struct AppState {
  layout_path: PathBuf,
  cached_layout: Arc<Mutex<ProjectLayout>>,
  stealth: Arc<Mutex<bool>>,
  desktop_visible: Arc<Mutex<bool>>,
}

#[tauri::command]
fn load_layout(state: State<'_, AppState>) -> Result<ProjectLayout, String> {
  let layout = read_layout(&state.layout_path).unwrap_or_else(|_| default_layout());
  let mut cache = state.cached_layout.lock().map_err(|_| "layout cache lock poisoned".to_string())?;
  *cache = layout.clone();
  Ok(layout)
}

#[tauri::command]
fn save_layout(state: State<'_, AppState>, layout: ProjectLayout) -> Result<(), String> {
  write_layout(&state.layout_path, &layout)?;
  let mut cache = state.cached_layout.lock().map_err(|_| "layout cache lock poisoned".to_string())?;
  *cache = layout;
  Ok(())
}

#[tauri::command]
fn launch_node(node: ProjectNode, action: String) -> Result<(), String> {
  match action.as_str() {
    "open-path" => {
      if let Some(path) = node.targets.path {
        open_target(&path)?;
      }
    }
    "open-editor" => {
      if let Some(path) = node.targets.editor.or(node.targets.path.clone()) {
        launch_code(&path)?;
      }
    }
    "open-browser" => {
      if let Some(url) = node.targets.browser {
        open_target(&url)?;
      }
    }
    "run-script" => {
      if let Some(script) = node.targets.script {
        run_script(&script, node.targets.path.as_deref())?;
      }
    }
    _ => return Err(format!("unknown action: {action}")),
  }

  Ok(())
}

#[tauri::command]
fn set_stealth_mode(app: AppHandle, window: Window, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
  update_stealth_mode(&app, &window, &state, enabled)
}

#[tauri::command]
fn set_desktop_visibility(app: AppHandle, state: State<'_, AppState>, visible: bool) -> Result<(), String> {
  apply_desktop_mode(&app, &state, visible)
}

#[tauri::command]
fn hide_main_window(window: Window) -> Result<(), String> {
  window.hide().map_err(|err| err.to_string())
}

#[tauri::command]
fn show_main_window(app: AppHandle) -> Result<(), String> {
  let window = app.get_window("main").ok_or_else(|| "main window not found".to_string())?;
  window.show().map_err(|err| err.to_string())?;
  window.unminimize().map_err(|err| err.to_string())?;
  window.set_focus().map_err(|err| err.to_string())
}

#[tauri::command]
fn show_settings_view(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
  apply_desktop_mode(&app, &state, false)?;
  let _ = app.emit_all("open-settings-tab", "general");
  Ok(())
}

fn apply_desktop_mode(app: &AppHandle, state: &State<'_, AppState>, visible: bool) -> Result<(), String> {
  {
    let mut desktop_visible = state
      .desktop_visible
      .lock()
      .map_err(|_| "desktop visibility lock poisoned".to_string())?;
    *desktop_visible = visible;
  }

  let window = app
    .get_window("main")
    .ok_or_else(|| "main window not found".to_string())?;

  window.show().map_err(|err| err.to_string())?;

  if visible {
    let _ = window.set_decorations(false);
    let _ = window.set_skip_taskbar(true);
    let _ = window.set_always_on_top(true);
    window.set_fullscreen(true).map_err(|err| err.to_string())?;
  } else {
    window.set_fullscreen(false).map_err(|err| err.to_string())?;
    let _ = window.set_always_on_top(false);
    let _ = window.set_skip_taskbar(false);
    let _ = window.set_decorations(true);
    let _ = window.unminimize();
    let _ = window.set_focus();
  }

  let _ = app.emit_all("desktop-visibility-changed", visible);
  Ok(())
}

#[tauri::command]
fn exit_app(app: AppHandle) {
  app.exit(0);
}

fn build_system_tray() -> SystemTray {
  let open_item = CustomMenuItem::new("open-settings", "Open Settings");
  let stealth_item = CustomMenuItem::new("toggle-stealth", "Toggle Stealth");
  let desktop_item = CustomMenuItem::new("toggle-desktop", "Show/Hide Desktop Nodes");
  let exit_item = CustomMenuItem::new("exit", "Exit");
  let tray_menu = SystemTrayMenu::new()
    .add_item(open_item)
    .add_item(stealth_item)
    .add_item(desktop_item)
    .add_native_item(SystemTrayMenuItem::Separator)
    .add_item(exit_item);

  SystemTray::new().with_menu(tray_menu)
}

fn main() {
  if let Err(error) = ensure_single_instance() {
    eprintln!("{error}");
    return;
  }

  tauri::Builder::default()
    .system_tray(build_system_tray())
    .manage(create_state())
    .on_system_tray_event(|app, event| match event {
      SystemTrayEvent::MenuItemClick { id, .. } => match id.as_str() {
        "open-settings" => {
          let state = app.state::<AppState>();
          let _ = show_settings_view(app.clone(), state);
        }
        "toggle-stealth" => {
          let state = app.state::<AppState>();
          let next_enabled = match state.stealth.lock() {
            Ok(mut stealth) => {
              *stealth = !*stealth;
              *stealth
            }
            Err(_) => {
              eprintln!("failed to toggle stealth mode from tray: lock poisoned");
              return;
            }
          };

          if let Some(window) = app.get_window("main") {
            let _ = apply_stealth_mode(&window, next_enabled);
          }

          let _ = app.emit_all("stealth-changed", next_enabled);
        }
        "toggle-desktop" => {
          let state = app.state::<AppState>();
          let next_visible = match state.desktop_visible.lock() {
            Ok(mut desktop_visible) => {
              *desktop_visible = !*desktop_visible;
              *desktop_visible
            }
            Err(_) => {
              eprintln!("failed to toggle desktop visibility from tray: lock poisoned");
              return;
            }
          };

          let _ = apply_desktop_mode(&app, &state, next_visible);
        }
        "exit" => app.exit(0),
        _ => {}
      },
      SystemTrayEvent::LeftClick { .. } | SystemTrayEvent::DoubleClick { .. } => {
        let state = app.state::<AppState>();
        let _ = show_settings_view(app.clone(), state);
      }
      _ => {}
    })
    .on_window_event(|event| {
      if event.window().label() != "main" {
        return;
      }

      if let WindowEvent::CloseRequested { api, .. } = event.event() {
        api.prevent_close();
        let _ = event.window().hide();
      }
    })
    .setup(|app| {
      let window = app.get_window("main").expect("main window");
      let state = app.state::<AppState>();
      let layout = read_layout(&state.layout_path).unwrap_or_else(|_| default_layout());
      let _ = write_layout(&state.layout_path, &layout);
      *state.cached_layout.lock().expect("cache") = layout;

      if let Err(error) = register_shortcuts(app.handle(), window.clone()) {
        eprintln!("failed to register global shortcut: {error}");
      }
      spawn_layout_watcher(app.handle(), state.layout_path.clone());
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      load_layout,
      save_layout,
      launch_node,
      set_stealth_mode,
      set_desktop_visibility,
      hide_main_window,
      show_main_window,
      show_settings_view,
      exit_app
    ])
    .run(tauri::generate_context!())
    .expect("error while running FinNode");
}

fn ensure_single_instance() -> Result<(), String> {
  let lock_dir = tauri::api::path::config_dir()
    .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
    .join("FinNode");
  fs::create_dir_all(&lock_dir).map_err(|err| err.to_string())?;

  let lock_path = lock_dir.join("app.lock");
  let file = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .open(&lock_path)
    .map_err(|err| err.to_string())?;

  file
    .try_lock_exclusive()
    .map_err(|_| "FinNode is already running. Close the existing app first.".to_string())?;

  let _ = APP_INSTANCE_LOCK.set(file);
  Ok(())
}

fn create_state() -> AppState {
  let base_dir = config_dir().unwrap_or_else(|_| {
    std::env::current_dir()
      .unwrap_or_else(|_| PathBuf::from("."))
      .join("FinNode")
  });
  let layout_path = base_dir.join("config.json");
  AppState {
    layout_path,
    cached_layout: Arc::new(Mutex::new(ProjectLayout::default())),
    stealth: Arc::new(Mutex::new(false)),
    desktop_visible: Arc::new(Mutex::new(false)),
  }
}

fn register_shortcuts(app: AppHandle, window: Window) -> Result<(), String> {
  let shortcut = "Alt+S";
  app.global_shortcut_manager()
    .register(shortcut, move || {
      let state = app.state::<AppState>();
      let next_enabled = match state.stealth.lock() {
        Ok(mut stealth) => {
          *stealth = !*stealth;
          *stealth
        }
        Err(_) => {
          eprintln!("failed to toggle stealth mode: lock poisoned");
          return;
        }
      };

      if let Err(error) = apply_stealth_mode(&window, next_enabled) {
        eprintln!("failed to update stealth mode: {error}");
        return;
      }

      let _ = app.emit_all("stealth-changed", next_enabled);
    })
    .map_err(|err| err.to_string())
}

fn spawn_layout_watcher(app: AppHandle, layout_path: PathBuf) {
  thread::spawn(move || {
    let path_for_event = layout_path.clone();
    let app_for_event = app.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
      if result.is_ok() {
        if let Ok(layout) = read_layout(&path_for_event) {
          let _ = app_for_event.emit_all("layout-updated", layout);
        }
      }
    })
    .expect("layout watcher");

    watcher
      .watch(&layout_path, RecursiveMode::NonRecursive)
      .expect("watch layout file");

    loop {
      thread::park();
    }
  });
}

fn default_layout() -> ProjectLayout {
  ProjectLayout {
    nodes: vec![
      ProjectNode {
        id: "core-reef".into(),
        name: "Core Reef".into(),
        icon: "⟐".into(),
        description: "Launches the main project folder and editor.".into(),
        x: 120.0,
        y: 110.0,
        links: vec!["tool-spine".into(), "signal-drift".into()],
        targets: LaunchTargets {
          path: Some(".".into()),
          editor: Some(".".into()),
          browser: Some("https://example.com".into()),
          script: Some("npm run build:web".into()),
        },
      },
      ProjectNode {
        id: "tool-spine".into(),
        name: "Tool Spine".into(),
        icon: "◈".into(),
        description: "Utilities, folders, and workspace shortcuts.".into(),
        x: 430.0,
        y: 210.0,
        links: vec!["research-fin".into()],
        targets: LaunchTargets {
          path: Some(".".into()),
          editor: Some(".".into()),
          browser: Some("https://github.com".into()),
          script: Some("npm run build:web".into()),
        },
      },
      ProjectNode {
        id: "research-fin".into(),
        name: "Research Fin".into(),
        icon: "⬡".into(),
        description: "A context node for docs, links, and references.".into(),
        x: 760.0,
        y: 140.0,
        links: vec!["signal-drift".into()],
        targets: LaunchTargets {
          path: Some(".".into()),
          editor: Some(".".into()),
          browser: Some("https://crates.io".into()),
          script: Some("cargo check".into()),
        },
      },
      ProjectNode {
        id: "signal-drift".into(),
        name: "Signal Drift".into(),
        icon: "⟁".into(),
        description: "A live node for scripts and browser targets.".into(),
        x: 540.0,
        y: 460.0,
        links: vec![],
        targets: LaunchTargets {
          path: Some(".".into()),
          editor: Some(".".into()),
          browser: Some("https://www.rust-lang.org".into()),
          script: Some("cargo fmt".into()),
        },
      },
    ],
  }
}

fn update_stealth_mode(app: &AppHandle, window: &Window, state: &State<'_, AppState>, enabled: bool) -> Result<(), String> {
  {
    let mut stealth = state.stealth.lock().map_err(|_| "stealth lock poisoned".to_string())?;
    *stealth = enabled;
  }

  apply_stealth_mode(window, enabled)?;
  let _ = app.emit_all("stealth-changed", enabled);
  Ok(())
}

fn apply_stealth_mode(window: &Window, enabled: bool) -> Result<(), String> {
  if enabled {
    window.hide().map_err(|err| err.to_string())?;
  } else {
    window.show().map_err(|err| err.to_string())?;
    let _ = window.unminimize();
    window.set_focus().map_err(|err| err.to_string())?;
  }

  Ok(())
}

fn config_dir() -> Result<PathBuf, String> {
  tauri::api::path::config_dir()
    .ok_or_else(|| "unable to resolve data directory".to_string())
    .map(|base| base.join("FinNode"))
}

fn read_layout(path: &Path) -> Result<ProjectLayout, String> {
  let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
  serde_json::from_str(&text).map_err(|err| err.to_string())
}

fn write_layout(path: &Path, layout: &ProjectLayout) -> Result<(), String> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).map_err(|err| err.to_string())?;
  }
  let text = serde_json::to_string_pretty(layout).map_err(|err| err.to_string())?;
  fs::write(path, text).map_err(|err| err.to_string())
}

fn open_target(target: &str) -> Result<(), String> {
  open::that(target).map(|_| ()).map_err(|err| err.to_string())
}

fn launch_code(path: &str) -> Result<(), String> {
  let candidates: &[&str] = if cfg!(target_os = "windows") {
    &["code.cmd", "code", "Code.exe", "code-insiders.cmd", "code-insiders"]
  } else {
    &["code", "code-insiders"]
  };

  for candidate in candidates {
    match Command::new(candidate).arg(path).spawn() {
      Ok(_) => return Ok(()),
      Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
      Err(err) => return Err(err.to_string()),
    }
  }

  open::that(path).map(|_| ()).map_err(|err| err.to_string())
}

fn run_script(script: &str, cwd: Option<&str>) -> Result<(), String> {
  let mut command = if cfg!(target_os = "windows") {
    let mut command = Command::new("cmd");
    command.args(["/C", script]);
    command
  } else {
    let mut command = Command::new("sh");
    command.args(["-lc", script]);
    command
  };

  if let Some(cwd) = cwd {
    command.current_dir(cwd);
  }

  command.spawn().map(|_| ()).map_err(|err| err.to_string())
}
