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

// ── Win32 FFI ────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
extern "system" {
    fn SetWindowPos(hwnd: isize, insert_after: isize, x: i32, y: i32, cx: i32, cy: i32, flags: u32) -> i32;
    fn GetWindowLongW(hwnd: isize, index: i32) -> i32;
    fn SetWindowLongW(hwnd: isize, index: i32, new_long: i32) -> i32;
    fn SystemParametersInfoW(ui_action: u32, ui_param: u32, pv_param: *mut WinRect, f_win_ini: u32) -> i32;
    // Subclassing: replace the window procedure
    fn SetWindowLongPtrW(hwnd: isize, index: i32, new_long: isize) -> isize;
    fn CallWindowProcW(prev_proc: isize, hwnd: isize, msg: u32, wparam: usize, lparam: isize) -> isize;
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct WinRect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}

#[cfg(target_os = "windows")]
const HWND_BOTTOM: isize = 1;
#[cfg(target_os = "windows")]
const SWP_NOACTIVATE: u32 = 0x0010;
#[cfg(target_os = "windows")]
const SWP_SHOWWINDOW: u32 = 0x0040;
#[cfg(target_os = "windows")]
const GWL_EXSTYLE: i32 = -20;
#[cfg(target_os = "windows")]
const WS_EX_NOACTIVATE: i32 = 0x08000000u32 as i32;
#[cfg(target_os = "windows")]
const WS_EX_TOOLWINDOW: i32 = 0x00000080;
#[cfg(target_os = "windows")]
const WS_EX_APPWINDOW: i32 = 0x00040000;
#[cfg(target_os = "windows")]
const SPI_GETWORKAREA: u32 = 0x0030;

// WM_NCHITTEST constants
#[cfg(target_os = "windows")]
const GWLP_WNDPROC: i32 = -4;
#[cfg(target_os = "windows")]
const WM_NCHITTEST: u32 = 0x0084;
#[cfg(target_os = "windows")]
const HTCLIENT: isize = 1;
#[cfg(target_os = "windows")]
const HTTRANSPARENT: isize = -1;

// ── Global State for WndProc ─────────────────────────────────────────────────
// The subclassed window procedure needs access to node bounds and settings.
// We use global statics since wndprocs can't capture closures.

#[cfg(target_os = "windows")]
static ORIGINAL_WNDPROC: OnceLock<isize> = OnceLock::new();
#[cfg(target_os = "windows")]
static WNDPROC_NODE_BOUNDS: OnceLock<Arc<Mutex<Vec<NodeBound>>>> = OnceLock::new();
#[cfg(target_os = "windows")]
static WNDPROC_CLICK_THROUGH: OnceLock<Arc<Mutex<bool>>> = OnceLock::new();

// ── Data Models ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MacroStep {
    action: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LaunchTargets {
    path: Option<String>,
    editor: Option<String>,
    browser: Option<String>,
    script: Option<String>,
}

/// Bounding box in physical screen-space pixels (sent from the frontend).
#[derive(Debug, Clone, Deserialize)]
struct NodeBound {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

#[derive(Debug, Clone, Serialize)]
struct DesktopHookStatus {
    ok: bool,
    detail: String,
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
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    group: Option<String>,
    #[serde(default)]
    macros: Vec<MacroStep>,
    #[serde(default)]
    collapsed: bool,
    #[serde(default)]
    last_launched: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Workspace {
    id: String,
    name: String,
    nodes: Vec<ProjectNode>,
    #[serde(default = "default_zoom")]
    zoom: f64,
    #[serde(default)]
    pan_x: f64,
    #[serde(default)]
    pan_y: f64,
}

fn default_zoom() -> f64 { 1.0 }

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HistoryEntry {
    timestamp: String,
    node_name: String,
    action: String,
    command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppLayout {
    #[serde(default = "default_workspace_id")]
    active_workspace: String,
    workspaces: Vec<Workspace>,
    #[serde(default)]
    command_history: Vec<HistoryEntry>,
}

fn default_workspace_id() -> String { "default".into() }

impl Default for AppLayout {
    fn default() -> Self {
        Self {
            active_workspace: "default".into(),
            workspaces: vec![default_workspace()],
            command_history: vec![],
        }
    }
}

// Legacy format for migration
#[derive(Debug, Deserialize)]
struct LegacyLayout {
    nodes: Vec<ProjectNode>,
}

// ── App State ────────────────────────────────────────────────────────────────

struct AppState {
    layout_path: PathBuf,
    cached_layout: Arc<Mutex<AppLayout>>,
    stealth: Arc<Mutex<bool>>,
    desktop_visible: Arc<Mutex<bool>>,
    desktop_click_through: Arc<Mutex<bool>>,
    /// Bounding boxes of all interactive elements (nodes, popups, etc.)
    node_bounds: Arc<Mutex<Vec<NodeBound>>>,
}

// ── Commands ─────────────────────────────────────────────────────────────────

#[tauri::command]
fn load_layout(state: State<'_, AppState>) -> Result<AppLayout, String> {
    let layout = read_layout(&state.layout_path).unwrap_or_default();
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    *cache = layout.clone();
    Ok(layout)
}

#[tauri::command]
fn save_layout(state: State<'_, AppState>, layout: AppLayout) -> Result<(), String> {
    write_layout(&state.layout_path, &layout)?;
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    *cache = layout;
    Ok(())
}

#[tauri::command]
fn launch_node(state: State<'_, AppState>, node: ProjectNode, action: String) -> Result<(), String> {
    let command_str = match action.as_str() {
        "open-path" => node.targets.path.clone().unwrap_or_default(),
        "open-editor" => node.targets.editor.clone().or(node.targets.path.clone()).unwrap_or_default(),
        "open-browser" => node.targets.browser.clone().unwrap_or_default(),
        "run-script" => node.targets.script.clone().unwrap_or_default(),
        _ => String::new(),
    };

    match action.as_str() {
        "open-path" => {
            if let Some(path) = &node.targets.path { open_target(path)?; }
        }
        "open-editor" => {
            if let Some(path) = node.targets.editor.as_ref().or(node.targets.path.as_ref()) {
                launch_code(path)?;
            }
        }
        "open-browser" => {
            if let Some(url) = &node.targets.browser { open_target(url)?; }
        }
        "run-script" => {
            if let Some(script) = &node.targets.script {
                run_script(script, node.targets.path.as_deref())?;
            }
        }
        _ => return Err(format!("unknown action: {action}")),
    }

    let entry = HistoryEntry {
        timestamp: chrono_now(),
        node_name: node.name.clone(),
        action: action.clone(),
        command: command_str,
    };
    if let Ok(mut cache) = state.cached_layout.lock() {
        cache.command_history.insert(0, entry);
        cache.command_history.truncate(50);
        let _ = write_layout(&state.layout_path, &cache);
    }

    Ok(())
}

#[tauri::command]
fn run_node_macro(steps: Vec<MacroStep>) -> Result<(), String> {
    thread::spawn(move || {
        for step in &steps {
            match step.action.as_str() {
                "open-path" | "open-browser" => { let _ = open_target(&step.value); }
                "run-script" => { let _ = run_script(&step.value, None); }
                "open-editor" => { let _ = launch_code(&step.value); }
                "delay" => {
                    let ms: u64 = step.value.parse().unwrap_or(1000);
                    thread::sleep(std::time::Duration::from_millis(ms));
                }
                _ => {}
            }
        }
    });
    Ok(())
}

#[tauri::command]
fn get_command_history(state: State<'_, AppState>) -> Result<Vec<HistoryEntry>, String> {
    let cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    Ok(cache.command_history.clone())
}

#[tauri::command]
fn clear_command_history(state: State<'_, AppState>) -> Result<(), String> {
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    cache.command_history.clear();
    let _ = write_layout(&state.layout_path, &cache);
    Ok(())
}

#[tauri::command]
fn list_workspaces(state: State<'_, AppState>) -> Result<Vec<Workspace>, String> {
    let cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    Ok(cache.workspaces.clone())
}

#[tauri::command]
fn create_workspace(state: State<'_, AppState>, name: String) -> Result<Workspace, String> {
    let ws = Workspace {
        id: format!("ws-{}", rand_id()),
        name,
        nodes: vec![],
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
    };
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    cache.workspaces.push(ws.clone());
    let _ = write_layout(&state.layout_path, &cache);
    Ok(ws)
}

#[tauri::command]
fn switch_workspace(state: State<'_, AppState>, workspace_id: String) -> Result<AppLayout, String> {
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    if !cache.workspaces.iter().any(|w| w.id == workspace_id) {
        return Err("workspace not found".into());
    }
    cache.active_workspace = workspace_id;
    let _ = write_layout(&state.layout_path, &cache);
    Ok(cache.clone())
}

#[tauri::command]
fn delete_workspace(state: State<'_, AppState>, workspace_id: String) -> Result<(), String> {
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    if cache.workspaces.len() <= 1 {
        return Err("cannot delete last workspace".into());
    }
    cache.workspaces.retain(|w| w.id != workspace_id);
    if cache.active_workspace == workspace_id {
        cache.active_workspace = cache.workspaces[0].id.clone();
    }
    let _ = write_layout(&state.layout_path, &cache);
    Ok(())
}

#[tauri::command]
fn rename_workspace(state: State<'_, AppState>, workspace_id: String, name: String) -> Result<(), String> {
    let mut cache = state.cached_layout.lock().map_err(|_| "lock poisoned")?;
    if let Some(ws) = cache.workspaces.iter_mut().find(|w| w.id == workspace_id) {
        ws.name = name;
    }
    let _ = write_layout(&state.layout_path, &cache);
    Ok(())
}

#[tauri::command]
fn set_stealth_mode(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    update_stealth_mode(&app, &state, enabled)
}

#[tauri::command]
fn set_desktop_visibility(app: AppHandle, state: State<'_, AppState>, visible: bool) -> Result<(), String> {
    apply_desktop_mode(&app, &state, visible)
}

#[tauri::command]
fn get_platform() -> String {
    std::env::consts::OS.to_string()
}

#[tauri::command]
fn set_desktop_click_through(app: AppHandle, state: State<'_, AppState>, enabled: bool) -> Result<(), String> {
    update_desktop_click_through(&app, &state, enabled)
}

/// Frontend sends the screen-space bounding boxes of all interactive elements.
/// The subclassed WM_NCHITTEST handler reads these to decide hit-test results.
#[tauri::command]
fn update_node_bounds(state: State<'_, AppState>, bounds: Vec<NodeBound>) -> Result<(), String> {
    let mut nb = state.node_bounds.lock().map_err(|_| "lock poisoned")?;
    *nb = bounds;
    Ok(())
}

#[tauri::command]
fn hide_main_window(window: Window) -> Result<(), String> {
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_main_window(app: AppHandle) -> Result<(), String> {
    let window = app.get_window("main").ok_or("main window not found")?;
    let _ = window.set_decorations(false);
    let _ = window.set_resizable(false);
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
fn show_settings_view(app: AppHandle, _state: State<'_, AppState>) -> Result<(), String> {
    show_main_window(app.clone())?;
    let _ = app.emit_all("open-settings-tab", "general");
    Ok(())
}

#[tauri::command]
fn pin_desktop_bottom(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    if let Some(desktop) = app.get_window("desktop") {
        set_window_bottom(&desktop)?;
    }
    Ok(())
}

#[tauri::command]
fn exit_app(app: AppHandle) {
    app.exit(0);
}

// ── Win32 Helpers ────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn setup_desktop_widget(window: &Window) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let raw = hwnd.0 as isize;

    unsafe {
        let ex = GetWindowLongW(raw, GWL_EXSTYLE);
        let new_ex = (ex | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
        SetWindowLongW(raw, GWL_EXSTYLE, new_ex);
    }

    // Size to Windows work area so the taskbar stays visible.
    let mut applied = false;
    unsafe {
        let mut rect = WinRect { left: 0, top: 0, right: 0, bottom: 0 };
        if SystemParametersInfoW(SPI_GETWORKAREA, 0, &mut rect as *mut WinRect, 0) != 0 {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 0 && height > 0 {
                SetWindowPos(
                    raw, HWND_BOTTOM,
                    rect.left, rect.top,
                    width, height,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
                applied = true;
            }
        }
    }

    if !applied {
        if let Ok(Some(monitor)) = window.primary_monitor() {
            let size = monitor.size();
            let pos = monitor.position();
            unsafe {
                SetWindowPos(
                    raw, HWND_BOTTOM,
                    pos.x as i32, pos.y as i32,
                    size.width as i32, size.height as i32,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn set_window_bottom(window: &Window) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let raw = hwnd.0 as isize;
    unsafe {
        SetWindowPos(raw, HWND_BOTTOM, 0, 0, 0, 0, SWP_NOACTIVATE | 0x0001 | 0x0002);
    }
    Ok(())
}

// ── WM_NCHITTEST Subclassing ─────────────────────────────────────────────────
// This is the core click-through mechanism. Instead of polling or toggling
// set_ignore_cursor_events, we intercept WM_NCHITTEST directly in the
// Windows message loop.
//
// For every mouse event, Windows asks our window: "Is this point yours?"
// We answer:
//   HTTRANSPARENT (-1) → "No, pass this click to whatever is behind me"
//   (default)          → "Yes, I want this click" → WebView2 gets it
//
// This is event-driven, zero-latency, and deterministic.

#[cfg(target_os = "windows")]
unsafe extern "system" fn desktop_wndproc(
    hwnd: isize,
    msg: u32,
    wparam: usize,
    lparam: isize,
) -> isize {
    let orig = ORIGINAL_WNDPROC.get().copied().unwrap_or(0);
    let default_result = if orig != 0 {
        CallWindowProcW(orig, hwnd, msg, wparam, lparam)
    } else {
        0
    };

    if msg == WM_NCHITTEST {
        // Preserve native non-client behavior (title bar, resize borders, etc.)
        // so decorated windows remain freely movable and resizable.
        if default_result != HTCLIENT {
            return default_result;
        }

        // Check if click-through is enabled in settings
        let ct_enabled = WNDPROC_CLICK_THROUGH
            .get()
            .and_then(|ct| ct.lock().ok())
            .map(|ct| *ct)
            .unwrap_or(false);

        if ct_enabled {
            // Extract screen coordinates from LPARAM (handles multi-monitor negative coords)
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            // Check if cursor is within any interactive bounding box
            let over_interactive = WNDPROC_NODE_BOUNDS
                .get()
                .and_then(|bounds| bounds.lock().ok())
                .map(|bounds| {
                    let pad = 6.0; // small padding for easier targeting
                    bounds.iter().any(|b| {
                        (x as f64) >= (b.left - pad)
                            && (x as f64) <= (b.right + pad)
                            && (y as f64) >= (b.top - pad)
                            && (y as f64) <= (b.bottom + pad)
                    })
                })
                .unwrap_or(false);

            if !over_interactive {
                // Not over a node → click passes through to desktop/apps below
                return HTTRANSPARENT;
            }
        }
        // Over a node or click-through disabled → fall through to WebView2
    }

    default_result
}

/// Replace the desktop window's message procedure with our custom one.
/// Must be called AFTER the window is shown and configured.
#[cfg(target_os = "windows")]
fn subclass_desktop_window(
    window: &Window,
    node_bounds: Arc<Mutex<Vec<NodeBound>>>,
    click_through: Arc<Mutex<bool>>,
) -> Result<(), String> {
    let hwnd = window.hwnd().map_err(|e| e.to_string())?;
    let raw = hwnd.0 as isize;

    if raw == 0 {
        return Err("invalid desktop window handle".into());
    }

    // Store shared state in globals so the wndproc can access them
    let _ = WNDPROC_NODE_BOUNDS.get_or_init(|| node_bounds);
    let _ = WNDPROC_CLICK_THROUGH.get_or_init(|| click_through);

    // Already hooked for this process.
    if ORIGINAL_WNDPROC.get().is_some() {
        return Ok(());
    }

    // Replace the window procedure, saving the original
    let original = unsafe {
        SetWindowLongPtrW(raw, GWLP_WNDPROC, desktop_wndproc as *const () as isize)
    };

    if original == 0 {
        return Err("SetWindowLongPtrW failed to subclass window".into());
    }

    let _ = ORIGINAL_WNDPROC.set(original);
    Ok(())
}

fn emit_desktop_hook_status(app: &AppHandle, ok: bool, detail: impl Into<String>) {
    let payload = DesktopHookStatus {
        ok,
        detail: detail.into(),
    };
    let _ = app.emit_all("desktop-hook-status", payload);
}

fn request_node_bounds_sync(app: &AppHandle) {
    let _ = app.emit_all("request-bounds-update", true);
}

#[cfg(target_os = "windows")]
fn ensure_desktop_hit_test_hook(
    app: &AppHandle,
    state: &State<'_, AppState>,
    desktop: &Window,
) -> Result<(), String> {
    match subclass_desktop_window(
        desktop,
        state.node_bounds.clone(),
        state.desktop_click_through.clone(),
    ) {
        Ok(()) => {
            emit_desktop_hook_status(app, true, "Desktop hit-test hook active");
            Ok(())
        }
        Err(e) => {
            emit_desktop_hook_status(app, false, format!("Desktop hit-test hook failed: {e}"));
            Err(e)
        }
    }
}

// ── Desktop Mode ─────────────────────────────────────────────────────────────

fn apply_desktop_mode(app: &AppHandle, state: &State<'_, AppState>, visible: bool) -> Result<(), String> {
    {
        let mut dv = state.desktop_visible.lock().map_err(|_| "lock poisoned")?;
        *dv = visible;
    }

    let desktop = app.get_window("desktop").ok_or("desktop window not found")?;

    if visible {
        let _ = desktop.set_decorations(true);
        let _ = desktop.set_skip_taskbar(true);
        let _ = desktop.set_always_on_top(false);
        let _ = desktop.set_resizable(true);
        desktop.show().map_err(|e| e.to_string())?;
        let _ = desktop.unminimize();

        #[cfg(target_os = "windows")]
        {
            if let Err(e) = ensure_desktop_hit_test_hook(app, state, &desktop) {
                eprintln!("failed to ensure desktop hit-test hook: {e}");
            }
            request_node_bounds_sync(app);
        }

        // WebView2 must ALWAYS be hit-test visible (never ignore cursor events).
        // The WM_NCHITTEST subclass handles click-through at the window level.
        let _ = desktop.set_ignore_cursor_events(false);
    } else {
        let _ = desktop.hide();
    }

    let _ = app.emit_all("desktop-visibility-changed", visible);
    Ok(())
}

fn update_desktop_click_through(app: &AppHandle, state: &State<'_, AppState>, enabled: bool) -> Result<(), String> {
    {
        let mut ct = state.desktop_click_through.lock().map_err(|_| "lock poisoned")?;
        *ct = enabled;
    }

    #[cfg(target_os = "windows")]
    if enabled {
        if let Some(desktop) = app.get_window("desktop") {
            if let Err(e) = ensure_desktop_hit_test_hook(app, state, &desktop) {
                eprintln!("failed to ensure desktop hit-test hook on click-through toggle: {e}");
            }
            request_node_bounds_sync(app);
        }
    }

    // The WM_NCHITTEST handler reads click_through state on every hit-test.
    // No need to toggle anything here — it's instantly effective.
    let _ = app.emit_all("desktop-click-through-changed", enabled);
    Ok(())
}

// ── Stealth Mode ─────────────────────────────────────────────────────────────

fn update_stealth_mode(app: &AppHandle, state: &State<'_, AppState>, enabled: bool) -> Result<(), String> {
    {
        let mut s = state.stealth.lock().map_err(|_| "lock poisoned")?;
        *s = enabled;
    }
    apply_stealth_mode(app, state, enabled)?;
    let _ = app.emit_all("stealth-changed", enabled);
    Ok(())
}

fn apply_stealth_mode(app: &AppHandle, state: &State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let main = app.get_window("main").ok_or("main window not found")?;
    let desktop = app.get_window("desktop").ok_or("desktop window not found")?;

    if enabled {
        let _ = main.hide();
        let _ = desktop.hide();
    } else {
        main.show().map_err(|e| e.to_string())?;
        let _ = main.unminimize();
        main.set_focus().map_err(|e| e.to_string())?;

        let dv = *state.desktop_visible.lock().map_err(|_| "lock poisoned")?;
        if dv {
            desktop.show().map_err(|e| e.to_string())?;
            let _ = desktop.unminimize();
            let _ = desktop.set_decorations(true);
            let _ = desktop.set_resizable(true);

            #[cfg(target_os = "windows")]
            {
                if let Err(e) = ensure_desktop_hit_test_hook(app, state, &desktop) {
                    eprintln!("failed to ensure desktop hit-test hook after stealth: {e}");
                }
                request_node_bounds_sync(app);
            }

            let _ = desktop.set_ignore_cursor_events(false);
        }
    }
    Ok(())
}

// ── Layout I/O ───────────────────────────────────────────────────────────────

fn read_layout(path: &Path) -> Result<AppLayout, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;

    if let Ok(layout) = serde_json::from_str::<AppLayout>(&text) {
        if !layout.workspaces.is_empty() {
            return Ok(layout);
        }
    }

    if let Ok(legacy) = serde_json::from_str::<LegacyLayout>(&text) {
        return Ok(AppLayout {
            active_workspace: "default".into(),
            workspaces: vec![Workspace {
                id: "default".into(),
                name: "Default".into(),
                nodes: legacy.nodes,
                zoom: 1.0,
                pan_x: 0.0,
                pan_y: 0.0,
            }],
            command_history: vec![],
        });
    }

    Err("failed to parse layout".into())
}

fn write_layout(path: &Path, layout: &AppLayout) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(layout).map_err(|e| e.to_string())?;
    fs::write(path, text).map_err(|e| e.to_string())
}

// ── Default Data ─────────────────────────────────────────────────────────────

fn default_workspace() -> Workspace {
    Workspace {
        id: "default".into(),
        name: "Default".into(),
        nodes: default_nodes(),
        zoom: 1.0,
        pan_x: 0.0,
        pan_y: 0.0,
    }
}

fn default_nodes() -> Vec<ProjectNode> {
    vec![
        ProjectNode {
            id: "core-reef".into(), name: "Core Reef".into(), icon: "⟐".into(),
            description: "Launches the main project folder and editor.".into(),
            x: 120.0, y: 110.0,
            links: vec!["tool-spine".into(), "signal-drift".into()],
            targets: LaunchTargets {
                path: Some(".".into()), editor: Some(".".into()),
                browser: Some("https://example.com".into()), script: Some("npm run build:web".into()),
            },
            color: None, group: None, macros: vec![], collapsed: false, last_launched: None,
        },
        ProjectNode {
            id: "tool-spine".into(), name: "Tool Spine".into(), icon: "◈".into(),
            description: "Utilities, folders, and workspace shortcuts.".into(),
            x: 430.0, y: 210.0,
            links: vec!["research-fin".into()],
            targets: LaunchTargets {
                path: Some(".".into()), editor: Some(".".into()),
                browser: Some("https://github.com".into()), script: Some("npm run build:web".into()),
            },
            color: None, group: None, macros: vec![], collapsed: false, last_launched: None,
        },
        ProjectNode {
            id: "research-fin".into(), name: "Research Fin".into(), icon: "⬡".into(),
            description: "A context node for docs, links, and references.".into(),
            x: 760.0, y: 140.0,
            links: vec!["signal-drift".into()],
            targets: LaunchTargets {
                path: Some(".".into()), editor: Some(".".into()),
                browser: Some("https://crates.io".into()), script: Some("cargo check".into()),
            },
            color: None, group: None, macros: vec![], collapsed: false, last_launched: None,
        },
        ProjectNode {
            id: "signal-drift".into(), name: "Signal Drift".into(), icon: "⟁".into(),
            description: "A live node for scripts and browser targets.".into(),
            x: 540.0, y: 460.0, links: vec![],
            targets: LaunchTargets {
                path: Some(".".into()), editor: Some(".".into()),
                browser: Some("https://www.rust-lang.org".into()), script: Some("cargo fmt".into()),
            },
            color: None, group: None, macros: vec![], collapsed: false, last_launched: None,
        },
    ]
}

// ── Utilities ────────────────────────────────────────────────────────────────

fn rand_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    format!("{:x}", n)
}

fn chrono_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

fn open_target(target: &str) -> Result<(), String> {
    open::that(target).map(|_| ()).map_err(|e| e.to_string())
}

fn launch_code(path: &str) -> Result<(), String> {
    let candidates: &[&str] = if cfg!(target_os = "windows") {
        &["code.cmd", "code", "Code.exe", "code-insiders.cmd", "code-insiders"]
    } else {
        &["code", "code-insiders"]
    };
    for c in candidates {
        match Command::new(c).arg(path).spawn() {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
    open::that(path).map(|_| ()).map_err(|e| e.to_string())
}

fn run_script(script: &str, cwd: Option<&str>) -> Result<(), String> {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = Command::new("cmd");
        c.args(["/C", script]);
        c
    } else {
        let mut c = Command::new("sh");
        c.args(["-lc", script]);
        c
    };
    if let Some(cwd) = cwd { cmd.current_dir(cwd); }
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

fn config_dir() -> Result<PathBuf, String> {
    tauri::api::path::config_dir()
        .ok_or_else(|| "unable to resolve data directory".into())
        .map(|base| base.join("FinNode"))
}

fn ensure_single_instance() -> Result<(), String> {
    let lock_dir = tauri::api::path::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .join("FinNode");
    fs::create_dir_all(&lock_dir).map_err(|e| e.to_string())?;
    let lock_path = lock_dir.join("app.lock");
    let file = OpenOptions::new().read(true).write(true).create(true)
        .open(&lock_path).map_err(|e| e.to_string())?;
    file.try_lock_exclusive()
        .map_err(|_| "FinNode is already running. Close the existing app first.".to_string())?;
    let _ = APP_INSTANCE_LOCK.set(file);
    Ok(())
}

fn create_state() -> AppState {
    let base_dir = config_dir().unwrap_or_else(|_| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")).join("FinNode")
    });
    AppState {
        layout_path: base_dir.join("config.json"),
        cached_layout: Arc::new(Mutex::new(AppLayout::default())),
        stealth: Arc::new(Mutex::new(false)),
        desktop_visible: Arc::new(Mutex::new(true)),
        desktop_click_through: Arc::new(Mutex::new(true)),
        node_bounds: Arc::new(Mutex::new(Vec::new())),
    }
}

// ── System Tray ──────────────────────────────────────────────────────────────

fn build_system_tray() -> SystemTray {
    let menu = SystemTrayMenu::new()
        .add_item(CustomMenuItem::new("open-settings", "Open Settings"))
        .add_item(CustomMenuItem::new("toggle-stealth", "Toggle Stealth"))
        .add_item(CustomMenuItem::new("toggle-desktop", "Show/Hide Desktop Nodes"))
        .add_item(CustomMenuItem::new("toggle-click-through", "Toggle Background Click-Through"))
        .add_native_item(SystemTrayMenuItem::Separator)
        .add_item(CustomMenuItem::new("exit", "Exit"));
    SystemTray::new().with_menu(menu)
}

// ── Shortcuts ────────────────────────────────────────────────────────────────

fn register_shortcuts(app: AppHandle) -> Result<(), String> {
    let app1 = app.clone();
    app.global_shortcut_manager()
        .register("Alt+S", move || {
            let state = app1.state::<AppState>();
            let next = match state.stealth.lock() {
                Ok(mut s) => { *s = !*s; *s }
                Err(_) => return,
            };
            let _ = update_stealth_mode(&app1, &state, next);
        })
        .map_err(|e| e.to_string())?;

    let app2 = app.clone();
    app.global_shortcut_manager()
        .register("Alt+I", move || {
            let state = app2.state::<AppState>();
            let next = match state.desktop_click_through.lock() {
                Ok(ct) => !*ct,
                Err(_) => return,
            };
            let _ = update_desktop_click_through(&app2, &state, next);
        })
        .map_err(|e| e.to_string())?;

    let app3 = app.clone();
    app.global_shortcut_manager()
        .register("Alt+Space", move || {
            let _ = app3.emit_all("toggle-quick-launcher", true);
        })
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ── Layout Watcher ───────────────────────────────────────────────────────────

fn spawn_layout_watcher(app: AppHandle, layout_path: PathBuf) {
    thread::spawn(move || {
        let path = layout_path.clone();
        let app_ref = app.clone();
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
            if result.is_ok() {
                if let Ok(layout) = read_layout(&path) {
                    let _ = app_ref.emit_all("layout-updated", layout);
                }
            }
        }).expect("layout watcher");
        watcher.watch(&layout_path, RecursiveMode::NonRecursive).expect("watch layout file");
        loop { thread::park(); }
    });
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() {
    if let Err(e) = ensure_single_instance() {
        eprintln!("{e}");
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
                    let next = match state.stealth.lock() {
                        Ok(mut s) => { *s = !*s; *s }
                        Err(_) => return,
                    };
                    let _ = update_stealth_mode(app, &state, next);
                }
                "toggle-desktop" => {
                    let state = app.state::<AppState>();
                    let next = match state.desktop_visible.lock() {
                        Ok(dv) => !*dv,
                        Err(_) => return,
                    };
                    let _ = apply_desktop_mode(app, &state, next);
                }
                "toggle-click-through" => {
                    let state = app.state::<AppState>();
                    let next = match state.desktop_click_through.lock() {
                        Ok(ct) => !*ct,
                        Err(_) => return,
                    };
                    let _ = update_desktop_click_through(app, &state, next);
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
            if let WindowEvent::CloseRequested { api, .. } = event.event() {
                api.prevent_close();
                if event.window().label() == "desktop" {
                    let app = event.window().app_handle();
                    let state = app.state::<AppState>();
                    let _ = apply_desktop_mode(&app, &state, false);
                    return;
                }
                if event.window().label() == "main" {
                    let _ = event.window().hide();
                }
            }
        })
        .setup(|app| {
            let window = app.get_window("main").expect("main window");
            let _ = window.set_resizable(false);
            let _ = window.set_decorations(false);
            let app_handle = app.handle();

            let state = app.state::<AppState>();
            let layout = read_layout(&state.layout_path).unwrap_or_default();
            let _ = write_layout(&state.layout_path, &layout);
            *state.cached_layout.lock().expect("cache") = layout;

            // Startup policy: background click-through starts enabled.
            let _ = update_desktop_click_through(&app_handle, &state, true);

            if let Err(e) = register_shortcuts(app.handle()) {
                eprintln!("failed to register shortcuts: {e}");
            }
            spawn_layout_watcher(app_handle.clone(), state.layout_path.clone());

            // Auto-show desktop overlay as widget
            if let Some(desktop) = app.get_window("desktop") {
                let _ = desktop.set_decorations(true);
                let _ = desktop.set_skip_taskbar(true);
                let _ = desktop.set_always_on_top(false);
                let _ = desktop.set_resizable(true);
                let _ = desktop.show();

                // Keep WebView2 hit-test visible (WM_NCHITTEST handles click-through)
                let _ = desktop.set_ignore_cursor_events(false);

                #[cfg(target_os = "windows")]
                {
                    if let Err(e) = ensure_desktop_hit_test_hook(&app_handle, &state, &desktop) {
                        eprintln!("failed to ensure desktop hit-test hook: {e}");
                    }
                    request_node_bounds_sync(&app_handle);
                }
            }

            let _ = window.show();

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_layout, save_layout, launch_node, run_node_macro,
            set_stealth_mode, set_desktop_visibility, set_desktop_click_through,
            update_node_bounds,
            get_platform,
            hide_main_window, show_main_window, show_settings_view, exit_app,
            pin_desktop_bottom,
            list_workspaces, create_workspace, switch_workspace, delete_workspace, rename_workspace,
            get_command_history, clear_command_history
        ])
        .run(tauri::generate_context!())
        .expect("error while running FinNode");
}
