//! 计时服务（Rust 侧）
//!
//! - 总计时器：按系统时间戳计算，窗口隐藏 / 后台也准；
//! - 停止后**不落库**，先挂在 `pending`，等用户在 F3 面板点「保存」才写库。

use chrono::Local;
use serde::Serialize;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 正在计时
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub project_id: i64,
    pub task_id: i64,
    pub started_at: String,
    /// 开始时刻（毫秒时间戳）
    pub started_ms: i64,
}

/// 停止后待确认的阶段
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingStage {
    pub project_id: i64,
    pub task_id: i64,
    pub started_at: String,
    pub ended_at: String,
    pub duration: i64,
}

#[derive(Default)]
pub struct TimerState {
    pub running: Mutex<Option<Running>>,
    pub pending: Mutex<Option<PendingStage>>,
}

/// 开始计时
pub fn start(app: &AppHandle, project_id: i64, task_id: i64) -> Result<(), String> {
    let state = app.state::<TimerState>();
    let mut g = state.running.lock().map_err(|_| "计时状态锁异常".to_string())?;
    if g.is_some() {
        return Err("已经在计时了".into());
    }
    let now = Local::now();
    *g = Some(Running {
        project_id,
        task_id,
        started_at: now.format("%Y-%m-%d %H:%M:%S").to_string(),
        started_ms: now.timestamp_millis(),
    });
    drop(g);

    // 每秒推一次心跳给前端
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let st = handle.state::<TimerState>();
        let running = match st.running.lock() {
            Ok(g) => g.clone(),
            Err(_) => break,
        };
        match running {
            Some(r) => {
                let elapsed = ((Local::now().timestamp_millis() - r.started_ms) / 1000).max(0);
                let _ = handle.emit("timer-tick", elapsed);
            }
            None => break,
        }
    });
    Ok(())
}

/// 停止计时 → 返回待保存的阶段（不落库）
pub fn stop(app: &AppHandle) -> Result<PendingStage, String> {
    let state = app.state::<TimerState>();
    let mut g = state.running.lock().map_err(|_| "计时状态锁异常".to_string())?;
    let running = g.take().ok_or_else(|| "当前没有在计时".to_string())?;
    let ended = Local::now();
    let pending = PendingStage {
        project_id: running.project_id,
        task_id: running.task_id,
        started_at: running.started_at.clone(),
        ended_at: ended.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration: ((ended.timestamp_millis() - running.started_ms) / 1000).max(0),
    };
    let mut p = state.pending.lock().map_err(|_| "计时状态锁异常".to_string())?;
    *p = Some(pending.clone());
    Ok(pending)
}

/// 丢弃待保存阶段（F3 点「删除记录」）
pub fn discard(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<TimerState>();
    let mut p = state.pending.lock().map_err(|_| "计时状态锁异常".to_string())?;
    *p = None;
    Ok(())
}

/// 当前状态（前端刷新后恢复显示用）
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerInfo {
    pub running: bool,
    pub elapsed: i64,
    pub project_id: Option<i64>,
    pub task_id: Option<i64>,
    pub has_pending: bool,
}

pub fn info(app: &AppHandle) -> TimerInfo {
    let state = app.state::<TimerState>();
    let running = state.running.lock().ok().and_then(|g| g.clone());
    let has_pending = state.pending.lock().map(|g| g.is_some()).unwrap_or(false);
    let elapsed = running
        .as_ref()
        .map(|r| ((Local::now().timestamp_millis() - r.started_ms) / 1000).max(0))
        .unwrap_or(0);
    TimerInfo {
        running: running.is_some(),
        elapsed,
        project_id: running.as_ref().map(|r| r.project_id),
        task_id: running.as_ref().map(|r| r.task_id),
        has_pending,
    }
}
