//! workLog · 桌面端核心
//!
//! 双窗口：`clip`（悬浮球，小窗口常驻） + `board`（主面板，隐藏 / 显示）。
//! 悬浮球拖动时主面板跟随，展开 / 收起不再 resize 窗口。

mod commands;
mod config;
mod db;
mod export;
mod report;
mod stats;
mod timer;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WindowEvent};

/// 让主面板跟随悬浮球：
/// - 水平：悬浮球中心对齐主面板中心
/// - 垂直：主面板上边缘落在悬浮球中央水平线
pub(crate) fn sync_board_position(app: &AppHandle) -> Result<(), String> {
    let clip = app
        .get_webview_window("clip")
        .ok_or_else(|| "悬浮球窗口不存在".to_string())?;
    let cp = clip.outer_position().map_err(|e| e.to_string())?;
    sync_board_at(app, cp)
}

/// 用已知的悬浮球位置同步主面板，避免拖动时每条 Moved 事件都做多次 WindowServer 查询。
pub(crate) fn sync_board_at(app: &AppHandle, cp: PhysicalPosition<i32>) -> Result<(), String> {
    let board = app
        .get_webview_window("board")
        .ok_or_else(|| "主面板窗口不存在".to_string())?;
    let st = app.state::<UiState>();
    let (cw, _ch) = *st.clip_size.lock().map_err(|_| "窗口状态锁异常".to_string())?;
    let (bw, _bh) = *st.board_size.lock().map_err(|_| "窗口状态锁异常".to_string())?;
    let scale = *st.scale.lock().map_err(|_| "窗口状态锁异常".to_string())?;
    let x = cp.x + cw as i32 / 2 - bw as i32 / 2;
    // 垂直位置固定在「悬浮球第一行中心」：计时态变高时主面板不跟着上下跳
    let y = cp.y + (31.0 * scale).round() as i32;
    board
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|e| format!("同步主面板位置失败：{e}"))?;
    Ok(())
}

pub(crate) fn set_clip_size(app: &AppHandle, width: u32, height: u32) {
    if let Ok(mut s) = app.state::<UiState>().clip_size.lock() {
        *s = (width, height);
    }
}

/// macOS：让悬浮球窗口始终位于主面板之上（两者都在普通应用之上）
#[cfg(target_os = "macos")]
fn set_window_levels(app: &AppHandle) {
    use objc2_app_kit::{NSFloatingWindowLevel, NSWindow};
    if let Some(board) = app.get_webview_window("board") {
        if let Ok(ptr) = board.ns_window() {
            let win = unsafe { &*(ptr as *mut NSWindow) };
            win.setLevel(NSFloatingWindowLevel);
        }
    }
    if let Some(clip) = app.get_webview_window("clip") {
        if let Ok(ptr) = clip.ns_window() {
            let win = unsafe { &*(ptr as *mut NSWindow) };
            win.setLevel(NSFloatingWindowLevel + 1);
        }
    }
    link_board_to_clip(app);
}

/// macOS：把主面板作为悬浮球的子窗口，并排在悬浮球下方。
/// 这样拖动悬浮球时系统自动带着主面板走，不存在 IPC 跟随延迟。
#[cfg(target_os = "macos")]
pub(crate) fn link_board_to_clip(app: &AppHandle) {
    use objc2_app_kit::{NSWindow, NSWindowOrderingMode};
    let (Some(clip), Some(board)) = (
        app.get_webview_window("clip"),
        app.get_webview_window("board"),
    ) else {
        return;
    };
    if let (Ok(cp), Ok(bp)) = (clip.ns_window(), board.ns_window()) {
        let clip_win = unsafe { &*(cp as *mut NSWindow) };
        let board_win = unsafe { &*(bp as *mut NSWindow) };
        unsafe {
            clip_win.addChildWindow_ordered(board_win, NSWindowOrderingMode::Below);
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn unlink_board_from_clip(app: &AppHandle) {
    use objc2_app_kit::NSWindow;
    let (Some(clip), Some(board)) = (
        app.get_webview_window("clip"),
        app.get_webview_window("board"),
    ) else {
        return;
    };
    if let (Ok(cp), Ok(bp)) = (clip.ns_window(), board.ns_window()) {
        let clip_win = unsafe { &*(cp as *mut NSWindow) };
        let board_win = unsafe { &*(bp as *mut NSWindow) };
        clip_win.removeChildWindow(board_win);
    }
}

#[cfg(not(target_os = "macos"))]
fn set_window_levels(_app: &AppHandle) {}

/// Windows：主面板显示 / 重新置顶后，把悬浮球重新提到最上层，避免被板子盖住一半
#[cfg(target_os = "windows")]
pub(crate) fn raise_clip(app: &AppHandle) {
    use std::ffi::c_void;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };
    if let Some(clip) = app.get_webview_window("clip") {
        if let Ok(hwnd) = clip.hwnd() {
            unsafe {
                SetWindowPos(
                    hwnd.0 as *mut c_void,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    }
}

/// 托盘隐藏 / 显示时记住主面板是否打开着
#[derive(Default)]
struct UiState {
    board_was_visible: Mutex<bool>,
    /// 悬浮球窗口尺寸（物理像素），拖动时复用，避免反复查询
    clip_size: Mutex<(u32, u32)>,
    /// 主面板窗口尺寸（物理像素）
    board_size: Mutex<(u32, u32)>,
    scale: Mutex<f64>,
}

pub(crate) fn set_board_visible(app: &AppHandle, visible: bool) {
    if let Ok(mut w) = app.state::<UiState>().board_was_visible.lock() {
        *w = visible;
    }
}

fn hide_all(app: &AppHandle) {
    let state = app.state::<UiState>();
    if let Some(board) = app.get_webview_window("board") {
        if let Ok(mut w) = state.board_was_visible.lock() {
            *w = board.is_visible().unwrap_or(false);
        }
        let _ = board.hide();
    }
    if let Some(clip) = app.get_webview_window("clip") {
        let _ = clip.hide();
    }
}

fn show_all(app: &AppHandle) {
    let state = app.state::<UiState>();
    let restore_board = state
        .board_was_visible
        .lock()
        .map(|g| *g)
        .unwrap_or(false);
    if let Some(clip) = app.get_webview_window("clip") {
        let _ = clip.show();
    }
    if restore_board {
        let _ = sync_board_position(app);
        if let Some(board) = app.get_webview_window("board") {
            let _ = board.show();
        }
        #[cfg(target_os = "macos")]
        link_board_to_clip(app);
        #[cfg(target_os = "windows")]
        raise_clip(app);
    }
}

/// 托盘菜单：显示 / 隐藏、开始 / 停止、退出
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
    let toggle = MenuItem::with_id(app, "toggle", "开始 / 停止计时", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &toggle, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("workLog")
        .menu(&menu)
        .on_menu_event(|tray, event| {
            let app = tray.app_handle();
            match event.id().as_ref() {
                "show" => {
                    let visible = app
                        .get_webview_window("clip")
                        .and_then(|w| w.is_visible().ok())
                        .unwrap_or(false);
                    if visible {
                        hide_all(&app);
                    } else {
                        show_all(&app);
                    }
                }
                "toggle" => {
                    if let Some(clip) = app.get_webview_window("clip") {
                        let _ = clip.show();
                    }
                    let _ = app.emit("tray-toggle-timer", ());
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            use tauri::tray::TrayIconEvent;
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                show_all(&app);
            }
        });

    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 单实例：再次启动时只把已有悬浮球显示出来，不再开第二个
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(clip) = app.get_webview_window("clip") {
                let _ = clip.show();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            config::init(&handle).map_err(std::io::Error::other)?;
            db::init(&handle).map_err(std::io::Error::other)?;
            app.manage(timer::TimerState::default());
            timer::restore(&handle);
            app.manage(UiState::default());

            set_window_levels(&handle);

            // 缓存两窗尺寸 / 缩放，拖动同步时不再逐次查询
            {
                let st = handle.state::<UiState>();
                if let Some(clip) = app.get_webview_window("clip") {
                    if let Ok(sz) = clip.outer_size() {
                        *st.clip_size.lock().unwrap() = (sz.width, sz.height);
                    }
                    if let Ok(sc) = clip.scale_factor() {
                        *st.scale.lock().unwrap() = sc;
                    }
                }
                if let Some(board) = app.get_webview_window("board") {
                    if let Ok(sz) = board.outer_size() {
                        *st.board_size.lock().unwrap() = (sz.width, sz.height);
                    }
                }
            }

            // 还原悬浮球上次位置，并把主面板摆到正确位置
            let cfg = config::load(&handle);
            if cfg.window.saved {
                if let Some(clip) = app.get_webview_window("clip") {
                    let _ = clip.set_position(PhysicalPosition::new(cfg.window.x, cfg.window.y));
                }
            }
            let _ = sync_board_position(&handle);
            // 挂成 child window 会让隐藏的主面板变成可显示状态，这里重新收回
            if let Some(board) = app.get_webview_window("board") {
                let _ = board.hide();
            }

            build_tray(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            let label = window.label().to_string();
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    // 关闭窗口不退出进程（常驻托盘）
                    api.prevent_close();
                    if label == "clip" {
                        hide_all(window.app_handle());
                    } else {
                        let _ = window.hide();
                        set_board_visible(window.app_handle(), false);
                        let _ = window.app_handle().emit("board-force-closed", ());
                    }
                }
                WindowEvent::Moved(pos) if label == "clip" => {
                    #[cfg(not(target_os = "macos"))]
                    {
                        let _ = sync_board_at(window.app_handle(), *pos);
                    }
                    #[cfg(target_os = "macos")]
                    let _ = pos;
                }
                WindowEvent::Resized(size) if label == "clip" => {
                    set_clip_size(window.app_handle(), size.width, size.height);
                    let _ = sync_board_position(window.app_handle());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::get_task,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::list_stages,
            commands::add_stage,
            commands::update_stage,
            commands::delete_stage,
            commands::start_timer,
            commands::stop_timer,
            commands::discard_pending,
            commands::timer_state,
            commands::keep_working,
            commands::get_summary,
            commands::generate_report,
            commands::ai_rewrite_report,
            commands::reset_report_config,
            commands::get_config,
            commands::save_config,
            commands::save_window_pos,
            commands::show_board,
            commands::hide_board,
            commands::resize_clip_window,
            commands::open_config_file,
            commands::app_info,
            export::export_tasks,
            export::open_data_dir,
        ])
        .run(tauri::generate_context!())
        .expect("workLog 启动失败");
}
