//! 给前端调用的命令（`invoke('xxx', {...})`）

use crate::config::{self, Config};
use crate::db::{project, stage, Db};
use crate::report::{self, ReportResult};
use crate::stats::{self, Summary};
use crate::timer::{self, PendingStage, TimerInfo};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

/// 应用信息（版本、数据库位置等）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub db_path: String,
    pub config_dir: String,
    pub template_path: String,
    pub skill_path: String,
}

#[tauri::command]
pub fn app_info(app: AppHandle) -> Result<AppInfo, String> {
    let db_path = app
        .path()
        .app_data_dir()
        .map(|d| d.join("worklog.db").to_string_lossy().to_string())
        .unwrap_or_default();
    let config_dir = config::config_dir(&app)?.to_string_lossy().to_string();
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        db_path,
        config_dir,
        template_path: config::report_file(&app, "template")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        skill_path: config::report_file(&app, "skill")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
    })
}

/* ---------------- 数据表 / 详情 ---------------- */

#[tauri::command]
pub fn list_tasks(db: State<'_, Db>) -> Result<Vec<project::TaskRow>, String> {
    project::list(&db)
}

#[tauri::command]
pub fn get_task(db: State<'_, Db>, project_id: i64, task_id: i64) -> Result<project::TaskDetail, String> {
    project::get(&db, project_id, task_id)
}

#[tauri::command]
pub fn create_task(db: State<'_, Db>, input: project::TaskInput) -> Result<project::TaskRow, String> {
    project::create(&db, &input)
}

#[tauri::command]
pub fn update_task(
    db: State<'_, Db>,
    project_id: i64,
    task_id: i64,
    input: project::TaskInput,
) -> Result<project::TaskIdentity, String> {
    project::update(&db, project_id, task_id, &input)
}

#[tauri::command]
pub fn delete_task(db: State<'_, Db>, project_id: i64, task_id: i64) -> Result<(), String> {
    project::delete(&db, project_id, task_id)
}

#[tauri::command]
pub fn list_stages(db: State<'_, Db>, project_id: i64, task_id: i64) -> Result<Vec<stage::StageRow>, String> {
    stage::list(&db, project_id, task_id)
}

#[tauri::command]
pub fn add_stage(db: State<'_, Db>, input: stage::StageInput) -> Result<stage::StageRow, String> {
    stage::add(&db, &input)
}

#[tauri::command]
pub fn delete_stage(db: State<'_, Db>, project_id: i64, task_id: i64, stage_id: i64) -> Result<(), String> {
    stage::delete(&db, project_id, task_id, stage_id)
}

#[tauri::command]
pub fn update_stage(
    db: State<'_, Db>,
    project_id: i64,
    task_id: i64,
    stage_id: i64,
    patch: stage::StagePatch,
) -> Result<(), String> {
    stage::update(&db, project_id, task_id, stage_id, &patch)
}

/* ---------------- 统计 / 报告 ---------------- */

#[tauri::command]
pub fn get_summary(db: State<'_, Db>, period: String) -> Result<Summary, String> {
    stats::summary(&db, &period)
}

#[tauri::command]
pub fn generate_report(app: AppHandle, db: State<'_, Db>, period: String) -> Result<ReportResult, String> {
    report::generate(&app, &db, &period)
}

#[tauri::command]
pub async fn ai_rewrite_report(
    app: AppHandle,
    db: State<'_, Db>,
    period: String,
    draft: String,
) -> Result<String, String> {
    report::ai_rewrite(&app, &db, &period, &draft).await
}

#[tauri::command]
pub fn reset_report_config(app: AppHandle, kind: String) -> Result<(), String> {
    let kind = if kind == "skill" { "skill" } else { "template" };
    config::reset_report_file(&app, kind)
}

/* ---------------- 计时 ---------------- */

#[tauri::command]
pub fn start_timer(app: AppHandle, project_id: i64, task_id: i64) -> Result<(), String> {
    timer::start(&app, project_id, task_id)
}

#[tauri::command]
pub fn stop_timer(app: AppHandle) -> Result<PendingStage, String> {
    timer::stop(&app)
}

#[tauri::command]
pub fn discard_pending(app: AppHandle) -> Result<(), String> {
    timer::discard(&app)
}

#[tauri::command]
pub fn timer_state(app: AppHandle) -> Result<TimerInfo, String> {
    Ok(timer::info(&app))
}

#[tauri::command]
pub fn keep_working(app: AppHandle) -> Result<(), String> {
    timer::keep_working(&app)
}

/* ---------------- 配置 ---------------- */

#[tauri::command]
pub fn get_config(app: AppHandle) -> Result<Config, String> {
    Ok(config::load(&app))
}

#[tauri::command]
pub fn save_config(app: AppHandle, cfg: Config) -> Result<(), String> {
    config::save(&app, &cfg)
}

/// 显示主面板窗口：先按悬浮球位置摆好，再显示。
/// Windows 的窗口是不透明的，整块 body 都铺了面板色，直接 show 会「啪」地出现，
/// 所以这里从屏幕上方滑入；macOS / Linux 仍是透明窗口，由前端 CSS 完成下滑。
#[tauri::command]
pub fn show_board(app: AppHandle) -> Result<(), String> {
    let board = app
        .get_webview_window("board")
        .ok_or_else(|| "主面板窗口不存在".to_string())?;

    #[cfg(target_os = "windows")]
    {
        // 窗口直接摆到目标位置，用 Win32 region 从上往下展开；
        // 这样和 macOS 的裁切动画一致，也不会移动整块 WebView。
        crate::sync_board_position(&app)?;
        crate::clip_board_region(&app);
        board.show().map_err(|e| format!("显示主面板失败：{e}"))?;
        crate::raise_clip(&app);
        crate::animate_board_region(app.clone(), true, false);
    }

    #[cfg(not(target_os = "windows"))]
    {
        crate::sync_board_position(&app)?;
        board.show().map_err(|e| format!("显示主面板失败：{e}"))?;
    }

    #[cfg(target_os = "macos")]
    crate::link_board_to_clip(&app);
    crate::set_board_visible(&app, true);
    // macOS 通过窗口 level 保证悬浮球在上；Windows 用 SetWindowPos 重新提到 topmost 最前
    #[cfg(target_os = "windows")]
    crate::raise_clip_soon(&app, 90);
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    if let Some(clip) = app.get_webview_window("clip") {
        let _ = clip.show();
    }
    Ok(())
}

/// 隐藏主面板窗口
#[tauri::command]
pub fn hide_board(app: AppHandle) -> Result<(), String> {
    let board = app
        .get_webview_window("board")
        .ok_or_else(|| "主面板窗口不存在".to_string())?;
    if !board.is_visible().unwrap_or(false) {
        crate::set_board_visible(&app, false);
        return Ok(());
    }

    #[cfg(target_os = "windows")]
    {
        crate::set_board_visible(&app, false);
        crate::animate_board_region(app.clone(), false, true);
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        board.hide().map_err(|e| format!("隐藏主面板失败：{e}"))?;
        crate::set_board_visible(&app, false);
        Ok(())
    }
}

/// 悬浮球右键菜单：在鼠标位置弹出原生菜单
#[tauri::command]
pub fn show_clip_menu(app: AppHandle) -> Result<(), String> {
    let clip = app
        .get_webview_window("clip")
        .ok_or_else(|| "悬浮球窗口不存在".to_string())?;
    let menu = {
        let state = app.state::<crate::UiState>();
        let guard = state
            .clip_menu
            .lock()
            .map_err(|_| "菜单状态锁异常".to_string())?;
        guard
            .as_ref()
            .cloned()
            .ok_or_else(|| "悬浮球菜单未初始化".to_string())?
    };
    clip
        .popup_menu(&menu)
        .map_err(|e| format!("打开悬浮球菜单失败：{e}"))
}

/// 悬浮球内容变化时调整它自己的窗口大小（保持中心点不动）
#[tauri::command]
pub fn resize_clip_window(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let clip = app
        .get_webview_window("clip")
        .ok_or_else(|| "悬浮球窗口不存在".to_string())?;
    #[cfg(target_os = "macos")]
    let board = app.get_webview_window("board");
    #[cfg(target_os = "macos")]
    let board_visible = board.as_ref().and_then(|b| b.is_visible().ok()).unwrap_or(false);
    let pos = clip.outer_position().map_err(|e| e.to_string())?;
    let old = clip.outer_size().map_err(|e| e.to_string())?;
    let scale = clip.scale_factor().map_err(|e| e.to_string())?;
    // 子窗口会随父窗口一起移动；先解除，改完父窗口位置后再重新挂上，避免主面板被带着跳
    #[cfg(target_os = "macos")]
    crate::unlink_board_from_clip(&app);
    let new_w = (width * scale).round() as i32;
    // 先改大小再改位置：Windows 上 set_position 是异步的，先定位后改大小
    // 容易被系统按新宽度重新摆放，导致计时开始/停止时横向漂移。
    clip.set_size(tauri::LogicalSize::new(width, height))
        .map_err(|e| format!("调整悬浮球大小失败：{e}"))?;
    let x = pos.x + (old.width as i32 - new_w) / 2;
    clip.set_position(tauri::PhysicalPosition::new(x, pos.y))
        .map_err(|e| format!("调整悬浮球位置失败：{e}"))?;
    crate::set_clip_size(&app, new_w.max(1) as u32, (height * scale).round() as u32);
    crate::sync_board_position(&app)?;
    #[cfg(target_os = "macos")]
    {
        crate::link_board_to_clip(&app);
        if !board_visible {
            if let Some(board) = board {
                let _ = board.hide();
            }
        }
    }
    #[cfg(target_os = "windows")]
    crate::raise_clip(&app);
    Ok(())
}

/// 保存主窗体坐标
#[tauri::command]
pub fn save_window_pos(app: AppHandle, x: i32, y: i32) -> Result<(), String> {
    config::save_window_pos(&app, x, y)
}

/// 用系统默认程序打开文件（设置里的「打开」按钮）
#[tauri::command]
pub fn open_config_file(app: AppHandle, rel_path: String) -> Result<(), String> {
    if rel_path.contains("..") {
        return Err("路径不合法".into());
    }
    let dir = config::config_dir(&app)?;
    let path = dir.join(&rel_path);
    if !path.exists() {
        return Err(format!("文件不存在：{}", path.display()));
    }
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(path.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("打开失败：{e}"))?;
    Ok(())
}
