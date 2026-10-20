//! SQLite 落库：两张表（项目-任务维度 + 阶段明细）
//!
//! 维度字段只存一次，阶段明细随每次计时追加（PRD F4）。

pub mod project;
pub mod schema;
pub mod stage;

use rusqlite::Connection;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// 全局连接（SQLite 单连接 + 互斥，够用且简单）
pub struct Db(pub Mutex<Connection>);

fn db_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("取不到数据目录：{e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("建数据目录失败：{e}"))?;
    Ok(dir.join("worklog.db"))
}

/// 打开数据库并建表
pub fn init(app: &AppHandle) -> Result<(), String> {
    let file = db_file(app)?;
    let conn = Connection::open(&file).map_err(|e| format!("打开数据库失败：{e}"))?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| format!("设置 PRAGMA 失败：{e}"))?;
    schema::migrate(&conn).map_err(|e| format!("建表失败：{e}"))?;
    app.manage(Db(Mutex::new(conn)));
    Ok(())
}

/// 便捷取锁
pub fn with_conn<T>(db: &Db, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
    let guard = db.0.lock().map_err(|_| "数据库锁异常".to_string())?;
    f(&guard)
}
