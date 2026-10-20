//! 给前端调用的命令（`invoke('xxx', {...})`）

use crate::config::{self, Config};
use crate::db::{project, stage, Db};
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
) -> Result<(), String> {
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

/* ---------------- 配置 ---------------- */

#[tauri::command]
pub fn get_config(app: AppHandle) -> Result<Config, String> {
    Ok(config::load(&app))
}

#[tauri::command]
pub fn save_config(app: AppHandle, cfg: Config) -> Result<(), String> {
    config::save(&app, &cfg)
}

/// 用系统默认程序打开文件（设置里的「打开」按钮）
#[tauri::command]
pub fn open_config_file(app: AppHandle, rel_path: String) -> Result<(), String> {
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
