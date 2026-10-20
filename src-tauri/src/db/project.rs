//! 项目 - 任务维度表 CRUD

use super::with_conn;
use crate::db::Db;
use chrono::Local;
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

/// 数据表的一行
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskRow {
    /// 前端用的复合 id："project_id-task_id"
    pub id: String,
    pub project_id: i64,
    pub task_id: i64,
    pub project_name: String,
    pub task_name: String,
    pub due_date: Option<String>,
    pub priority: String,
    pub status: String,
    pub remark: String,
    pub custom_fields: String,
    pub updated_at: String,
    pub stage_count: i64,
    pub total_sec: i64,
}

/// 详情（字段 + 阶段列表）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub row: TaskRow,
    pub stages: Vec<super::stage::StageRow>,
}

/// 新建 / 修改的入参
#[derive(serde::Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub project_name: String,
    pub task_name: String,
    pub due_date: Option<String>,
    pub priority: String,
    pub status: Option<String>,
    pub remark: Option<String>,
    pub custom_fields: Option<String>,
}

fn row_from(r: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRow> {
    let project_id: i64 = r.get(0)?;
    let task_id: i64 = r.get(1)?;
    Ok(TaskRow {
        id: format!("{project_id}-{task_id}"),
        project_id,
        task_id,
        project_name: r.get(2)?,
        task_name: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
        due_date: r.get(4)?,
        priority: r.get(5)?,
        status: r.get(6)?,
        remark: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
        custom_fields: r.get::<_, Option<String>>(8)?.unwrap_or_else(|| "{}".into()),
        updated_at: r.get(9)?,
        stage_count: r.get(10)?,
        total_sec: r.get::<_, Option<i64>>(11)?.unwrap_or(0),
    })
}

const SELECT_TASK: &str = r#"
SELECT p.project_id, p.task_id, p.project_name, p.task_name, p.due_date, p.priority,
       p.status, p.remark, p.custom_fields, p.updated_at,
       (SELECT COUNT(*) FROM stage s WHERE s.project_id = p.project_id AND s.task_id = p.task_id),
       (SELECT COALESCE(SUM(s.duration), 0) FROM stage s WHERE s.project_id = p.project_id AND s.task_id = p.task_id)
FROM project_task p
"#;

/// 列表：默认按最近操作倒序
pub fn list(db: &Db) -> Result<Vec<TaskRow>, String> {
    with_conn(db, |conn| {
        let sql = format!("{SELECT_TASK} ORDER BY p.updated_at DESC, p.project_id DESC, p.task_id DESC");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], row_from)
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    })
}

/// 详情
pub fn get(db: &Db, project_id: i64, task_id: i64) -> Result<TaskDetail, String> {
    let stages = super::stage::list(db, project_id, task_id)?;
    with_conn(db, |conn| {
        let sql = format!("{SELECT_TASK} WHERE p.project_id = ?1 AND p.task_id = ?2");
        let mut stmt = conn.prepare(&sql).map_err(|e| e.to_string())?;
        let row = stmt
            .query_row(params![project_id, task_id], row_from)
            .optional()
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "任务不存在".to_string())?;
        Ok(TaskDetail { row, stages })
    })
}

/// 新建：项目号全局自增，任务号项目内自增
pub fn create(db: &Db, input: &TaskInput) -> Result<TaskRow, String> {
    with_conn(db, |conn| {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let project_name = input.project_name.trim();
        if project_name.is_empty() {
            return Err("项目名不能为空".into());
        }
        // 同名项目沿用已有 project_id（PRD：项目名即项目）
        let project_id: i64 = conn
            .query_row(
                "SELECT project_id FROM project_task WHERE project_name = ?1 ORDER BY project_id LIMIT 1",
                params![project_name],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| {
                conn.query_row("SELECT COALESCE(MAX(project_id),0)+1 FROM project_task", [], |r| r.get(0))
                    .unwrap_or(1)
            });
        let task_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(task_id),0)+1 FROM project_task WHERE project_id = ?1",
                params![project_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let priority = if ["高", "中", "低"].contains(&input.priority.as_str()) {
            input.priority.clone()
        } else {
            "中".into()
        };
        conn.execute(
            "INSERT INTO project_task (project_id, task_id, project_name, task_name, status, priority, due_date, created_at, updated_at, remark, custom_fields)
             VALUES (?1,?2,?3,?4,'未完成',?5,?6,?7,?7,?8,?9)",
            params![
                project_id,
                task_id,
                project_name,
                input.task_name.trim(),
                priority,
                input.due_date,
                now,
                input.remark,
                input.custom_fields.clone().unwrap_or_else(|| "{}".into())
            ],
        )
        .map_err(|e| format!("新建失败：{e}"))?;
        Ok(TaskRow {
            id: format!("{project_id}-{task_id}"),
            project_id,
            task_id,
            project_name: project_name.to_string(),
            task_name: input.task_name.trim().to_string(),
            due_date: input.due_date.clone(),
            priority,
            status: "未完成".into(),
            remark: input.remark.clone().unwrap_or_default(),
            custom_fields: input.custom_fields.clone().unwrap_or_else(|| "{}".into()),
            updated_at: now,
            stage_count: 0,
            total_sec: 0,
        })
    })
}

/// 保存详情（字段 + 任务状态）
pub fn update(
    db: &Db,
    project_id: i64,
    task_id: i64,
    input: &TaskInput,
) -> Result<(), String> {
    with_conn(db, |conn| {
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let status = input.status.clone().unwrap_or_else(|| "未完成".into());
        let status = if ["未完成", "已完成"].contains(&status.as_str()) {
            status
        } else {
            "未完成".into()
        };
        let priority = if ["高", "中", "低"].contains(&input.priority.as_str()) {
            input.priority.clone()
        } else {
            "中".into()
        };
        let n = conn
            .execute(
                "UPDATE project_task
                 SET project_name = ?3, task_name = ?4, status = ?5, priority = ?6, due_date = ?7,
                     remark = ?8, custom_fields = ?9, updated_at = ?10
                 WHERE project_id = ?1 AND task_id = ?2",
                params![
                    project_id,
                    task_id,
                    input.project_name.trim(),
                    input.task_name.trim(),
                    status,
                    priority,
                    input.due_date,
                    input.remark,
                    input.custom_fields.clone().unwrap_or_else(|| "{}".into()),
                    now
                ],
            )
            .map_err(|e| format!("保存失败：{e}"))?;
        if n == 0 {
            return Err("任务不存在".into());
        }
        Ok(())
    })
}

/// 删除任务（连同阶段）
pub fn delete(db: &Db, project_id: i64, task_id: i64) -> Result<(), String> {
    with_conn(db, |conn| {
        conn.execute(
            "DELETE FROM stage WHERE project_id = ?1 AND task_id = ?2",
            params![project_id, task_id],
        )
        .map_err(|e| e.to_string())?;
        conn.execute(
            "DELETE FROM project_task WHERE project_id = ?1 AND task_id = ?2",
            params![project_id, task_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}
