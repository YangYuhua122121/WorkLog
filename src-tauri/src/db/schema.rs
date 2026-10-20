//! 建表与迁移（PRAGMA user_version 做版本号）

use rusqlite::Connection;

const SCHEMA_VERSION: i64 = 1;

pub fn migrate(conn: &Connection) -> Result<(), rusqlite::Error> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version < 1 {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS project_task (
              project_id    INTEGER NOT NULL,
              task_id       INTEGER NOT NULL,
              project_name  TEXT    NOT NULL,
              task_name     TEXT,
              status        TEXT    NOT NULL DEFAULT '未完成' CHECK(status IN ('未完成','已完成')),
              priority      TEXT    NOT NULL DEFAULT '中'   CHECK(priority IN ('高','中','低')),
              due_date      DATE,
              created_at    DATETIME NOT NULL,
              updated_at    DATETIME NOT NULL,
              remark        TEXT,
              custom_fields TEXT,
              PRIMARY KEY (project_id, task_id)
            );

            CREATE TABLE IF NOT EXISTS stage (
              project_id INTEGER NOT NULL,
              task_id    INTEGER NOT NULL,
              stage_id   INTEGER NOT NULL,
              started_at DATETIME NOT NULL,
              ended_at   DATETIME,
              duration   INTEGER,
              result     TEXT,
              next_goal  TEXT,
              status     TEXT,
              PRIMARY KEY (project_id, task_id, stage_id),
              FOREIGN KEY (project_id, task_id) REFERENCES project_task(project_id, task_id)
            );

            CREATE INDEX IF NOT EXISTS idx_stage_ended ON stage(ended_at);
            CREATE INDEX IF NOT EXISTS idx_pt_updated ON project_task(updated_at DESC);
            "#,
        )?;
        conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    }
    Ok(())
}
