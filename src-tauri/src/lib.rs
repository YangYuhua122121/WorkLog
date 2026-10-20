//! workLog · 桌面端核心
//!
//! 分层：db（SQLite 落库）· timer（Rust 侧计时）· config（用户配置）· commands（给前端调）

mod commands;
mod config;
mod db;
mod timer;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::Manager;

/// 托盘菜单：显示 / 隐藏、退出
fn build_tray(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "显示 / 隐藏", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("workLog")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => {
                if let Some(win) = app.get_webview_window("main") {
                    match win.is_visible() {
                        Ok(true) => { let _ = win.hide(); }
                        _ => { let _ = win.show(); let _ = win.set_focus(); }
                    }
                }
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|app, event| {
            use tauri::tray::TrayIconEvent;
            if let TrayIconEvent::Click { button: tauri::tray::MouseButton::Left, button_state: tauri::tray::MouseButtonState::Up, .. } = event {
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
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
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            config::init(&handle).map_err(std::io::Error::other)?;
            db::init(&handle).map_err(std::io::Error::other)?;
            app.manage(timer::TimerState::default());
            build_tray(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_tasks,
            commands::get_task,
            commands::create_task,
            commands::update_task,
            commands::delete_task,
            commands::list_stages,
            commands::add_stage,
            commands::delete_stage,
            commands::start_timer,
            commands::stop_timer,
            commands::discard_pending,
            commands::timer_state,
            commands::get_config,
            commands::save_config,
            commands::open_config_file,
            commands::app_info,
        ])
        .run(tauri::generate_context!())
        .expect("workLog 启动失败");
}
