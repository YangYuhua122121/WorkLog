//! 数据导出（F4）：项目-任务清单 → Excel / CSV，走系统下载目录并自动定位文件。
//!
//! 首版不弹保存对话框：文件名带时间戳，导出后直接在文件管理器中选中，避免阻塞界面。

use crate::db::{project, Db};
use chrono::{Local, NaiveDate};
use rust_xlsxwriter::{Format, Workbook};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

fn headers() -> Vec<&'static str> {
    vec![
        "项目号",
        "任务号",
        "项目名",
        "任务名",
        "状态",
        "优先级",
        "截止日期",
        "DDL（天）",
        "创建时间",
        "最近操作时间",
        "累计时长（秒）",
        "阶段数",
    ]
}

fn row_values(r: &project::TaskRow) -> Vec<String> {
    let ddl = r
        .due_date
        .as_ref()
        .and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .map(|d| d.signed_duration_since(Local::now().date_naive()).num_days())
        .map(|n| n.to_string())
        .unwrap_or_default();
    vec![
        format!("P{:03}", r.project_id),
        format!("T{:02}", r.task_id),
        r.project_name.clone(),
        r.task_name.clone(),
        r.status.clone(),
        r.priority.clone(),
        r.due_date.clone().unwrap_or_default(),
        ddl,
        r.created_at.clone(),
        r.updated_at.clone(),
        r.total_sec.to_string(),
        r.stage_count.to_string(),
    ]
}

fn export_dir(app: &AppHandle) -> PathBuf {
    app.path()
        .download_dir()
        .or_else(|_| app.path().home_dir())
        .unwrap_or_else(|_| std::env::temp_dir())
}

fn csv_escape(v: &str) -> String {
    if v.contains(',') || v.contains('"') || v.contains('\n') || v.contains('\r') {
        format!("\"{}\"", v.replace('"', "\"\""))
    } else {
        v.to_string()
    }
}

/// 导出项目-任务清单，返回文件绝对路径
#[tauri::command]
pub async fn export_tasks(app: AppHandle, db: tauri::State<'_, Db>, format: String) -> Result<String, String> {
    let rows = project::list(&db)?;
    let dir = export_dir(&app);
    fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败：{e}"))?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let is_xlsx = format.eq_ignore_ascii_case("xlsx") || format.eq_ignore_ascii_case("excel");
    let path = dir.join(format!(
        "workLog-项目任务-{stamp}.{}",
        if is_xlsx { "xlsx" } else { "csv" }
    ));

    if is_xlsx {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("项目-任务").map_err(|e| e.to_string())?;
        let bold = Format::new().set_bold();
        for (col, h) in headers().iter().enumerate() {
            sheet
                .write_string_with_format(0, col as u16, *h, &bold)
                .map_err(|e| e.to_string())?;
        }
        for (i, r) in rows.iter().enumerate() {
            for (col, v) in row_values(r).iter().enumerate() {
                sheet
                    .write_string(i as u32 + 1, col as u16, v)
                    .map_err(|e| e.to_string())?;
            }
        }
        for (col, w) in [10.0, 8.0, 24.0, 24.0, 10.0, 8.0, 13.0, 10.0, 20.0, 20.0, 14.0, 8.0]
            .iter()
            .enumerate()
        {
            sheet
                .set_column_width(col as u16, *w)
                .map_err(|e| e.to_string())?;
        }
        workbook.save(&path).map_err(|e| format!("写入 Excel 失败：{e}"))?;
    } else {
        let mut text = String::from("\u{feff}");
        text.push_str(&headers().join(","));
        text.push('\n');
        for r in &rows {
            let line: Vec<String> = row_values(r).iter().map(|v| csv_escape(v)).collect();
            text.push_str(&line.join(","));
            text.push('\n');
        }
        fs::write(&path, text).map_err(|e| format!("写入 CSV 失败：{e}"))?;
    }

    // 导出后直接在文件管理器中选中文件
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.to_string_lossy().to_string())
}

/// 打开数据库所在目录（F4：查看数据库）
#[tauri::command]
pub fn open_data_dir(app: AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("取不到数据目录：{e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("数据目录不存在：{e}"))?;
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_path(dir.to_string_lossy().to_string(), None::<&str>)
        .map_err(|e| format!("打开目录失败：{e}"))
}

/* ---------------- 全量数据库导出（项目-任务 JOIN 阶段） ---------------- */

#[derive(Clone)]
struct FullRow {
    project_id: i64,
    task_id: i64,
    project_name: String,
    task_name: String,
    task_status: String,
    priority: String,
    due_date: String,
    created_at: String,
    updated_at: String,
    remark: String,
    custom_fields: String,
    stage_id: i64,
    started_at: String,
    ended_at: String,
    duration: i64,
    result: String,
    next_goal: String,
    stage_status: String,
}

fn full_headers() -> Vec<&'static str> {
    vec![
        "项目号", "任务号", "项目名", "任务名", "任务状态", "优先级", "截止日期",
        "任务创建时间", "任务更新时间", "任务备注", "自定义字段",
        "阶段号", "阶段开始", "阶段结束", "时长（秒）", "阶段成果", "下一目标", "阶段状态",
    ]
}

fn full_values(r: &FullRow) -> Vec<String> {
    vec![
        format!("P{:03}", r.project_id),
        format!("T{:02}", r.task_id),
        r.project_name.clone(),
        r.task_name.clone(),
        r.task_status.clone(),
        r.priority.clone(),
        r.due_date.clone(),
        r.created_at.clone(),
        r.updated_at.clone(),
        r.remark.clone(),
        r.custom_fields.clone(),
        r.stage_id.to_string(),
        r.started_at.clone(),
        r.ended_at.clone(),
        r.duration.to_string(),
        r.result.clone(),
        r.next_goal.clone(),
        r.stage_status.clone(),
    ]
}

/// 按日区间 + 项目名导出全量数据（project_task LEFT JOIN stage）
#[tauri::command]
pub async fn export_records(
    app: AppHandle,
    db: tauri::State<'_, Db>,
    start: String,
    end: String,
    project: String,
    format: String,
) -> Result<String, String> {
    let start_dt = format!("{} 00:00:00", start);
    let end_date = NaiveDate::parse_from_str(&end, "%Y-%m-%d")
        .map_err(|_| "结束日期格式不正确".to_string())?
        + chrono::Duration::days(1);
    let end_dt = format!("{} 00:00:00", end_date.format("%Y-%m-%d"));
    let project = project.trim();
    let rows: Vec<FullRow> = crate::db::with_conn(&db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT p.project_id, p.task_id, p.project_name, COALESCE(p.task_name,''),
                        p.status, p.priority, COALESCE(p.due_date,''), p.created_at, p.updated_at,
                        COALESCE(p.remark,''), COALESCE(p.custom_fields,'{}'),
                        s.stage_id, s.started_at, COALESCE(s.ended_at,''), COALESCE(s.duration,0),
                        COALESCE(s.result,''), COALESCE(s.next_goal,''), COALESCE(s.status,'')
                 FROM project_task p
                 JOIN stage s ON s.project_id = p.project_id AND s.task_id = p.task_id
                 WHERE s.ended_at IS NOT NULL
                   AND s.ended_at >= ?1 AND s.ended_at < ?2
                   AND (?3 = '' OR p.project_name = ?3)
                 ORDER BY p.project_id, p.task_id, s.stage_id",
            )
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let mapped = stmt
            .query_map(rusqlite::params![start_dt, end_dt, project], |r| {
                Ok(FullRow {
                    project_id: r.get(0)?,
                    task_id: r.get(1)?,
                    project_name: r.get(2)?,
                    task_name: r.get(3)?,
                    task_status: r.get(4)?,
                    priority: r.get(5)?,
                    due_date: r.get(6)?,
                    created_at: r.get(7)?,
                    updated_at: r.get(8)?,
                    remark: r.get(9)?,
                    custom_fields: r.get(10)?,
                    stage_id: r.get(11)?,
                    started_at: r.get(12)?,
                    ended_at: r.get(13)?,
                    duration: r.get(14)?,
                    result: r.get(15)?,
                    next_goal: r.get(16)?,
                    stage_status: r.get(17)?,
                })
            })
            .map_err(|e| e.to_string())?;
        for row in mapped {
            out.push(row.map_err(|e| e.to_string())?);
        }
        Ok(out)
    })?;

    let dir = export_dir(&app);
    fs::create_dir_all(&dir).map_err(|e| format!("创建导出目录失败：{e}"))?;
    let stamp = Local::now().format("%Y%m%d-%H%M%S");
    let is_xlsx = format.eq_ignore_ascii_case("xlsx") || format.eq_ignore_ascii_case("excel");
    let safe_project: String = if project.is_empty() {
        "全部项目".into()
    } else {
        project
            .chars()
            .map(|c| if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') { c } else { '_' })
            .collect()
    };
    let path = dir.join(format!(
        "workLog-全量数据-{}-{stamp}.{}",
        safe_project,
        if is_xlsx { "xlsx" } else { "csv" }
    ));
    let headers = full_headers();
    if is_xlsx {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.set_name("全量数据").map_err(|e| e.to_string())?;
        let bold = Format::new().set_bold();
        for (col, h) in headers.iter().enumerate() {
            sheet.write_string_with_format(0, col as u16, *h, &bold).map_err(|e| e.to_string())?;
        }
        for (i, row) in rows.iter().enumerate() {
            for (col, v) in full_values(row).iter().enumerate() {
                sheet.write_string(i as u32 + 1, col as u16, v).map_err(|e| e.to_string())?;
            }
        }
        for (col, w) in [10.0, 8.0, 22.0, 22.0, 10.0, 8.0, 13.0, 20.0, 20.0, 24.0, 28.0, 8.0, 20.0, 20.0, 12.0, 32.0, 32.0, 10.0].iter().enumerate() {
            sheet.set_column_width(col as u16, *w).map_err(|e| e.to_string())?;
        }
        workbook.save(&path).map_err(|e| format!("写入 Excel 失败：{e}"))?;
    } else {
        let mut text = String::from("\u{feff}");
        text.push_str(&headers.join(","));
        text.push('\n');
        for r in &rows {
            let line: Vec<String> = full_values(r).iter().map(|v| csv_escape(v)).collect();
            text.push_str(&line.join(","));
            text.push('\n');
        }
        fs::write(&path, text).map_err(|e| format!("写入 CSV 失败：{e}"))?;
    }
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().reveal_item_in_dir(&path);
    Ok(path.to_string_lossy().to_string())
}
