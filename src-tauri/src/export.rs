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
pub fn export_tasks(app: AppHandle, db: tauri::State<'_, Db>, format: String) -> Result<String, String> {
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
