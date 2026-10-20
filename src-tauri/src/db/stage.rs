//! 阶段明细表 CRUD

use super::with_conn;
use crate::db::Db;
use chrono::Local;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StageRow {
    pub project_id: i64,
    pub task_id: i64,
    pub stage_id: i64,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration: i64,
    pub result: String,
    pub next_goal: String,
    pub status: String,
}

/// 新增阶段（F3 手动保存时写入）
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StageInput {
    pub project_id: i64,
    pub task_id: i64,
    pub started_at: String,
    pub ended_at: String,
    pub duration: i64,
    pub result: Option<String>,
    pub next_goal: Option<String>,
    pub status: Option<String>,
}

/// 阶段编辑（详情面板里改成果 / 下一目标 / 状态）
#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct StagePatch {
    pub result: Option<String>,
    pub next_goal: Option<String>,
    pub status: Option<String>,
}

/// 阶段列表（按阶段号升序）
pub fn list(db: &Db, project_id: i64, task_id: i64) -> Result<Vec<StageRow>, String> {
    with_conn(db, |conn| {
        let mut stmt = conn
            .prepare(
                "SELECT project_id, task_id, stage_id, started_at, ended_at, duration, result, next_goal, status
                 FROM stage WHERE project_id = ?1 AND task_id = ?2 ORDER BY stage_id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![project_id, task_id], |r| {
                Ok(StageRow {
                    project_id: r.get(0)?,
                    task_id: r.get(1)?,
                    stage_id: r.get(2)?,
                    started_at: r.get(3)?,
                    ended_at: r.get(4)?,
                    duration: r.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    result: r.get::<_, Option<String>>(6)?.unwrap_or_default(),
                    next_goal: r.get::<_, Option<String>>(7)?.unwrap_or_default(),
                    status: r.get::<_, Option<String>>(8)?.unwrap_or_default(),
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    })
}

/// 写入一条阶段（阶段号项目 - 任务内自增）
pub fn add(db: &Db, input: &StageInput) -> Result<StageRow, String> {
    with_conn(db, |conn| {
        let stage_id: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(stage_id),0)+1 FROM stage WHERE project_id = ?1 AND task_id = ?2",
                params![input.project_id, input.task_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let exists: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM project_task WHERE project_id = ?1 AND task_id = ?2",
                params![input.project_id, input.task_id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            return Err("任务不存在".into());
        }
        conn.execute(
            "INSERT INTO stage (project_id, task_id, stage_id, started_at, ended_at, duration, result, next_goal, status)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                input.project_id,
                input.task_id,
                stage_id,
                input.started_at,
                input.ended_at,
                input.duration,
                input.result.clone().unwrap_or_default(),
                input.next_goal.clone().unwrap_or_default(),
                input.status.clone().unwrap_or_else(|| "未完成".into())
            ],
        )
        .map_err(|e| format!("保存阶段失败：{e}"))?;
        // 阶段保存也是一次「操作」：更新最近操作时间与任务状态（默认排序用）
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        let status = input.status.clone().unwrap_or_else(|| "未完成".into());
        let status = if ["未完成", "已完成"].contains(&status.as_str()) {
            status
        } else {
            "未完成".into()
        };
        conn.execute(
            "UPDATE project_task SET updated_at = ?3, status = ?4
             WHERE project_id = ?1 AND task_id = ?2",
            params![input.project_id, input.task_id, now, status],
        )
        .map_err(|e| format!("更新任务状态失败：{e}"))?;
        Ok(StageRow {
            project_id: input.project_id,
            task_id: input.task_id,
            stage_id,
            started_at: input.started_at.clone(),
            ended_at: Some(input.ended_at.clone()),
            duration: input.duration,
            result: input.result.clone().unwrap_or_default(),
            next_goal: input.next_goal.clone().unwrap_or_default(),
            status: input.status.clone().unwrap_or_else(|| "未完成".into()),
        })
    })
}

/// 编辑一条阶段（详情面板保存）
pub fn update(
    db: &Db,
    project_id: i64,
    task_id: i64,
    stage_id: i64,
    patch: &StagePatch,
) -> Result<(), String> {
    with_conn(db, |conn| {
        let n = conn
            .execute(
                "UPDATE stage
                 SET result = COALESCE(?4, result),
                     next_goal = COALESCE(?5, next_goal),
                     status = COALESCE(?6, status)
                 WHERE project_id = ?1 AND task_id = ?2 AND stage_id = ?3",
                params![project_id, task_id, stage_id, patch.result, patch.next_goal, patch.status],
            )
            .map_err(|e| format!("保存阶段失败：{e}"))?;
        if n == 0 {
            return Err("阶段不存在".into());
        }
        // 详情里改阶段也算一次「操作」，让该行按最近操作时间排到前面
        let now = Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        conn.execute(
            "UPDATE project_task SET updated_at = ?3 WHERE project_id = ?1 AND task_id = ?2",
            params![project_id, task_id, now],
        )
        .map_err(|e| format!("更新最近操作时间失败：{e}"))?;
        Ok(())
    })
}

/// 删除一条阶段
pub fn delete(db: &Db, project_id: i64, task_id: i64, stage_id: i64) -> Result<(), String> {
    with_conn(db, |conn| {
        conn.execute(
            "DELETE FROM stage WHERE project_id = ?1 AND task_id = ?2 AND stage_id = ?3",
            params![project_id, task_id, stage_id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })
}
