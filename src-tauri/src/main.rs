#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Deserialize;
use std::{
    path::PathBuf,
    process::Command,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
use tauri::{AppHandle, GlobalShortcutManager, Manager, State, Window, WindowEvent};

#[derive(Debug, Clone, Deserialize)]
struct MacroStep {
    action: String,
    #[serde(default)]
    value: String,
    #[serde(default)]
    shell: Option<String>,
}

struct AppState {
    click_through: Arc<Mutex<bool>>,
    overlay_visible: Arc<Mutex<bool>>,
}

#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
fn open_target(target: String) -> Result<(), String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return Err("target is empty".into());
    }
    open::that(trimmed).map(|_| ()).map_err(|e| e.to_string())
}

#[tauri::command]
fn run_command(command: String, shell: Option<String>, cwd: Option<String>) -> Result<(), String> {
    let script = command.trim();
    if script.is_empty() {
        return Err("command is empty".into());
    }
    spawn_shell_command(script, shell.as_deref(), cwd.as_deref())
}

#[tauri::command]
fn run_macro(steps: Vec<MacroStep>, cwd: Option<String>) -> Result<(), String> {
    if steps.is_empty() {
        return Ok(());
    }

    thread::spawn(move || {
        for step in steps {
            match step.action.as_str() {
                "run_command" => {
                    let _ = spawn_shell_command(step.value.trim(), step.shell.as_deref(), cwd.as_deref());
                }
                "open_target" => {
                    let value = step.value.trim();
                    if !value.is_empty() {
                        let _ = open::that(value);
                    }
                }
                "sleep" => {
                    let ms = step.value.trim().parse::<u64>().unwrap_or(600);
                    thread::sleep(Duration::from_millis(ms));
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[tauri::command]
fn get_overlay_click_through(state: State<'_, AppState>) -> Result<bool, String> {
    let enabled = state
        .click_through
        .lock()
        .map_err(|_| "click-through lock poisoned")?;
    Ok(*enabled)
}

#[tauri::command]
fn set_overlay_click_through(
    window: Window,
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<bool, String> {
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| e.to_string())?;

    {
        let mut current = state
            .click_through
            .lock()
            .map_err(|_| "click-through lock poisoned")?;
        *current = enabled;
    }

    let _ = window.app_handle().emit_all("overlay-click-through", enabled);
    Ok(enabled)
}

#[tauri::command]
fn toggle_overlay_click_through(window: Window, state: State<'_, AppState>) -> Result<bool, String> {
    let next = {
        let current = state
            .click_through
            .lock()
            .map_err(|_| "click-through lock poisoned")?;
        !*current
    };
    set_overlay_click_through(window, state, next)
}

#[tauri::command]
fn get_overlay_visibility(state: State<'_, AppState>) -> Result<bool, String> {
    let visible = state
        .overlay_visible
        .lock()
        .map_err(|_| "visibility lock poisoned")?;
    Ok(*visible)
}

#[tauri::command]
fn set_overlay_visibility(
    window: Window,
    state: State<'_, AppState>,
    visible: bool,
) -> Result<bool, String> {
    if visible {
        window.show().map_err(|e| e.to_string())?;
        let _ = window.unminimize();
    } else {
        window.hide().map_err(|e| e.to_string())?;
    }

    {
        let mut current = state
            .overlay_visible
            .lock()
            .map_err(|_| "visibility lock poisoned")?;
        *current = visible;
    }

    let _ = window.app_handle().emit_all("overlay-visibility", visible);
    Ok(visible)
}

#[tauri::command]
fn toggle_overlay_visibility(window: Window, state: State<'_, AppState>) -> Result<bool, String> {
    let next = {
        let current = state
            .overlay_visible
            .lock()
            .map_err(|_| "visibility lock poisoned")?;
        !*current
    };
    set_overlay_visibility(window, state, next)
}

fn spawn_shell_command(script: &str, shell: Option<&str>, cwd: Option<&str>) -> Result<(), String> {
    if script.trim().is_empty() {
        return Ok(());
    }

    let mut command = command_for_shell(shell.unwrap_or(default_shell()), script);
    if let Some(dir) = cwd {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            command.current_dir(PathBuf::from(trimmed));
        }
    }

    command
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to launch command: {e}"))
}

fn default_shell() -> &'static str {
    if cfg!(target_os = "windows") {
        "powershell"
    } else {
        "bash"
    }
}

fn command_for_shell(shell: &str, script: &str) -> Command {
    let normalized = shell.trim().to_ascii_lowercase();

    if cfg!(target_os = "windows") {
        match normalized.as_str() {
            "cmd" => {
                let mut cmd = Command::new("cmd");
                cmd.args(["/C", script]);
                cmd
            }
            "bash" => {
                let mut cmd = Command::new("bash");
                cmd.args(["-lc", script]);
                cmd
            }
            _ => {
                let mut cmd = Command::new("powershell");
                cmd.args([
                    "-NoProfile",
                    "-ExecutionPolicy",
                    "Bypass",
                    "-Command",
                    script,
                ]);
                cmd
            }
        }
    } else if normalized == "powershell" || normalized == "pwsh" {
        let mut cmd = Command::new("pwsh");
        cmd.args(["-NoProfile", "-Command", script]);
        cmd
    } else {
        let shell_program = if normalized.is_empty() {
            "bash"
        } else {
            normalized.as_str()
        };
        let mut cmd = Command::new(shell_program);
        cmd.args(["-lc", script]);
        cmd
    }
}

fn register_shortcuts(app: AppHandle) -> Result<(), String> {
    let click_app = app.clone();
    app.global_shortcut_manager()
        .register("Alt+Shift+O", move || {
            let state = click_app.state::<AppState>();
            let next = match state.click_through.lock() {
                Ok(mut current) => {
                    *current = !*current;
                    *current
                }
                Err(_) => return,
            };

            if let Some(window) = click_app.get_window("main") {
                let _ = window.set_ignore_cursor_events(next);
            }
            let _ = click_app.emit_all("overlay-click-through", next);
        })
        .map_err(|e| e.to_string())?;

    let visibility_app = app.clone();
    app.global_shortcut_manager()
        .register("Alt+Shift+H", move || {
            let state = visibility_app.state::<AppState>();
            let next = match state.overlay_visible.lock() {
                Ok(mut current) => {
                    *current = !*current;
                    *current
                }
                Err(_) => return,
            };

            if let Some(window) = visibility_app.get_window("main") {
                if next {
                    let _ = window.show();
                    let _ = window.unminimize();
                } else {
                    let _ = window.hide();
                }
            }
            let _ = visibility_app.emit_all("overlay-visibility", next);
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .manage(AppState {
            click_through: Arc::new(Mutex::new(false)),
            overlay_visible: Arc::new(Mutex::new(true)),
        })
        .setup(|app| {
            let main_window = app.get_window("main").ok_or("main window not found")?;
            let _ = main_window.set_decorations(false);
            let _ = main_window.set_resizable(false);
            let _ = main_window.set_always_on_top(true);
            let _ = main_window.set_skip_taskbar(true);
            let _ = main_window.set_fullscreen(true);
            let _ = main_window.show();
            let _ = main_window.set_ignore_cursor_events(false);

            if let Err(err) = register_shortcuts(app.handle()) {
                eprintln!("failed to register shortcuts: {err}");
            }

            Ok(())
        })
        .on_window_event(|event| {
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                api.prevent_close();
                let _ = event.window().hide();

                let app = event.window().app_handle();
                let state = app.state::<AppState>();
                if let Ok(mut visible) = state.overlay_visible.lock() {
                    *visible = false;
                }

                let _ = app.emit_all("overlay-visibility", false);
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_platform,
            open_target,
            run_command,
            run_macro,
            get_overlay_click_through,
            set_overlay_click_through,
            toggle_overlay_click_through,
            get_overlay_visibility,
            set_overlay_visibility,
            toggle_overlay_visibility
        ])
        .run(tauri::generate_context!())
        .expect("error while running FinNode");
}
