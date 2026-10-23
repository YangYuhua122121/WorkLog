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
    pub created_at: String,
    pub updated_at: String,
    pub stage_count: i64,
    pub total_sec: i64,
}

/// 详情（字段 + 阶段列表）
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct TaskIdentity {
    pub project_id: i64,
    pub task_id: i64,
}

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
        created_at: r.get(9)?,
        updated_at: r.get(10)?,
        stage_count: r.get(11)?,
        total_sec: r.get::<_, Option<i64>>(12)?.unwrap_or(0),
    })
}

const SELECT_TASK: &str = r#"
SELECT p.project_id, p.task_id, p.project_name, p.task_name, p.due_date, p.priority,
       p.status, p.remark, p.custom_fields, p.created_at, p.updated_at,
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
            created_at: now.clone(),
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
) -> Result<TaskIdentity, String> {
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
        let project_name = input.project_name.trim();
        if project_name.is_empty() {
            return Err("项目名不能为空".into());
        }

        conn.execute("BEGIN IMMEDIATE", [])
            .map_err(|e| format!("开始保存事务失败：{e}"))?;
        // 任务换项目时 stage 的外键会短暂指向旧主键，提交时再统一校验
        let _ = conn.execute("PRAGMA defer_foreign_keys = ON", []);
        let result: Result<TaskIdentity, String> = (|| {
            let current_name: String = conn
                .query_row(
                    "SELECT project_name FROM project_task WHERE project_id = ?1 AND task_id = ?2",
                    params![project_id, task_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "任务不存在".to_string())?;

            // 项目名变了：把这条任务独立出去（或并入已有的同名项目），并重新编号。
            // 项目号 / 任务号是主键，stage 也要跟着迁移。
            let (target_project, target_task) = if project_name == current_name {
                (project_id, task_id)
            } else {
                let existing: Option<i64> = conn
                    .query_row(
                        "SELECT project_id FROM project_task
                         WHERE project_name = ?1 AND project_id <> ?2
                         ORDER BY project_id LIMIT 1",
                        params![project_name, project_id],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if let Some(pid) = existing {
                    let next: i64 = conn
                        .query_row(
                            "SELECT COALESCE(MAX(task_id), 0) + 1 FROM project_task WHERE project_id = ?1",
                            params![pid],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    (pid, next)
                } else {
                    let pid: i64 = conn
                        .query_row(
                            "SELECT COALESCE(MAX(project_id), 0) + 1 FROM project_task",
                            [],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    (pid, 1)
                }
            };

            if target_project != project_id || target_task != task_id {
                conn.execute(
                    "UPDATE project_task
                     SET project_id = ?1, task_id = ?2, project_name = ?3, task_name = ?4,
                         status = ?5, priority = ?6, due_date = ?7, remark = ?8,
                         custom_fields = ?9, updated_at = ?10
                     WHERE project_id = ?11 AND task_id = ?12",
                    params![
                        target_project,
                        target_task,
                        project_name,
                        input.task_name.trim(),
                        status,
                        priority,
                        input.due_date,
                        input.remark,
                        input.custom_fields.clone().unwrap_or_else(|| "{}".into()),
                        now,
                        project_id,
                        task_id
                    ],
                )
                .map_err(|e| format!("保存失败：{e}"))?;
                conn.execute(
                    "UPDATE stage SET project_id = ?1, task_id = ?2
                     WHERE project_id = ?3 AND task_id = ?4",
                    params![target_project, target_task, project_id, task_id],
                )
                .map_err(|e| format!("迁移阶段记录失败：{e}"))?;
            } else {
                conn.execute(
                    "UPDATE project_task
                     SET project_name = ?3, task_name = ?4, status = ?5, priority = ?6,
                         due_date = ?7, remark = ?8, custom_fields = ?9, updated_at = ?10
                     WHERE project_id = ?1 AND task_id = ?2",
                    params![
                        project_id,
                        task_id,
                        project_name,
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
            }
            Ok(TaskIdentity {
                project_id: target_project,
                task_id: target_task,
            })
        })();
        match result {
            Ok(v) => {
                conn.execute("COMMIT", [])
                    .map_err(|e| format!("提交保存失败：{e}"))?;
                Ok(v)
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                Err(e)
            }
        }
    })
}

/// 合并历史上重复的项目名：同一名称只保留最小 project_id，
/// 其余项目下的任务 / 阶段迁移过去并重新编号（防止汇总出现两个同名项目）。
pub fn merge_duplicate_projects(conn: &rusqlite::Connection) -> Result<(), String> {
    let mut stmt = conn
        .prepare(
            "SELECT project_name, GROUP_CONCAT(project_id)
             FROM (SELECT DISTINCT project_name, project_id FROM project_task)
             GROUP BY project_name HAVING COUNT(*) > 1",
        )
        .map_err(|e| e.to_string())?;
    let groups: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    drop(stmt);
    for (_name, csv) in groups {
        let mut ids: Vec<i64> = csv
            .split(',')
            .filter_map(|x| x.trim().parse().ok())
            .collect();
        ids.sort_unstable();
        ids.dedup();
        if ids.len() < 2 {
            continue;
        }
        let canonical = ids[0];
        conn.execute("BEGIN IMMEDIATE", [])
            .map_err(|e| format!("合并项目事务失败：{e}"))?;
        let _ = conn.execute("PRAGMA defer_foreign_keys = ON", []);
        let result: Result<(), String> = (|| {
            for pid in ids.iter().skip(1) {
                let task_ids: Vec<i64> = {
                    let mut s = conn
                        .prepare("SELECT task_id FROM project_task WHERE project_id = ?1 ORDER BY task_id")
                        .map_err(|e| e.to_string())?;
                    let rows = s
                        .query_map(params![pid], |r| r.get(0))
                        .map_err(|e| e.to_string())?
                        .collect::<Result<Vec<i64>, _>>()
                        .map_err(|e| e.to_string())?;
                    rows
                };
                for tid in task_ids {
                    let next: i64 = conn
                        .query_row(
                            "SELECT COALESCE(MAX(task_id), 0) + 1 FROM project_task WHERE project_id = ?1",
                            params![canonical],
                            |r| r.get(0),
                        )
                        .map_err(|e| e.to_string())?;
                    conn.execute(
                        "UPDATE project_task SET project_id = ?1, task_id = ?2
                         WHERE project_id = ?3 AND task_id = ?4",
                        params![canonical, next, pid, tid],
                    )
                    .map_err(|e| format!("迁移任务失败：{e}"))?;
                    conn.execute(
                        "UPDATE stage SET project_id = ?1, task_id = ?2
                         WHERE project_id = ?3 AND task_id = ?4",
                        params![canonical, next, pid, tid],
                    )
                    .map_err(|e| format!("迁移阶段失败：{e}"))?;
                }
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                conn.execute("COMMIT", [])
                    .map_err(|e| format!("提交合并失败：{e}"))?;
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK", []);
                return Err(e);
            }
        }
    }
    Ok(())
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


#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema;

    fn input(name: &str) -> TaskInput {
        TaskInput {
            project_name: name.into(),
            task_name: "T2".into(),
            due_date: None,
            priority: "中".into(),
            status: Some("未完成".into()),
            remark: None,
            custom_fields: Some("{}".into()),
        }
    }

    #[test]
    fn renaming_project_moves_and_renumbers_task() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys=ON", []).unwrap();
        schema::migrate(&conn).unwrap();
        for (pid, tid, name) in [(1, 1, "P1"), (1, 2, "P1")] {
            conn.execute(
                "INSERT INTO project_task
                 (project_id, task_id, project_name, task_name, status, priority, created_at, updated_at, custom_fields)
                 VALUES (?1, ?2, ?3, ?4, '未完成', '中', '2026-10-20 09:00:00', '2026-10-20 09:00:00', '{}')",
                params![pid, tid, name, format!("T{tid}")],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO stage VALUES (1, 2, 1, '2026-10-20 09:00:00', '2026-10-20 10:00:00', 3600, 'r', 'n', '未完成')",
            [],
        )
        .unwrap();
        let db = Db(std::sync::Mutex::new(conn));
        let moved = update(&db, 1, 2, &input("P2")).unwrap();
        assert_eq!((moved.project_id, moved.task_id), (2, 1));
        let conn = db.0.lock().unwrap();
        let remain: i64 = conn
            .query_row("SELECT COUNT(*) FROM project_task WHERE project_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remain, 1, "A 任务应留在原项目");
        let name: String = conn
            .query_row("SELECT project_name FROM project_task WHERE project_id=2 AND task_id=1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(name, "P2");
        let stage_owner: (i64, i64) = conn
            .query_row("SELECT project_id, task_id FROM stage WHERE stage_id=1", [], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(stage_owner, (2, 1), "阶段记录应随任务迁移");
    }
}


#[cfg(test)]
mod merge_tests {
    use super::*;
    use crate::db::schema;

    #[test]
    fn merges_same_name_projects() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute("PRAGMA foreign_keys=ON", []).unwrap();
        schema::migrate(&conn).unwrap();
        for (pid, tid) in [(1, 1), (1, 2), (2, 1), (2, 2)] {
            conn.execute(
                "INSERT INTO project_task
                 (project_id, task_id, project_name, task_name, status, priority, created_at, updated_at, custom_fields)
                 VALUES (?1, ?2, '重复项目', 'T', '未完成', '中', '2026-10-20 09:00:00', '2026-10-20 09:00:00', '{}')",
                params![pid, tid],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO stage VALUES (2, 2, 1, '2026-10-20 09:00:00', '2026-10-20 10:00:00', 3600, 'r', 'n', '未完成')",
            [],
        )
        .unwrap();
        merge_duplicate_projects(&conn).unwrap();
        let pids: i64 = conn
            .query_row("SELECT COUNT(DISTINCT project_id) FROM project_task", [], |r| r.get(0))
            .unwrap();
        assert_eq!(pids, 1);
        let tasks: i64 = conn.query_row("SELECT COUNT(*) FROM project_task", [], |r| r.get(0)).unwrap();
        assert_eq!(tasks, 4);
        let stage_owner: i64 = conn
            .query_row("SELECT project_id FROM stage WHERE stage_id = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(stage_owner, 1);
    }
}
