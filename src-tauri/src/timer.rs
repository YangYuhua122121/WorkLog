//! 计时服务（Rust 侧）
//!
//! - 总计时器：按系统时间戳计算，窗口隐藏 / 后台也准；
//! - 停止后**不落库**，先挂在 `pending`，等用户在 F3 面板点「保存」才写库；
//! - 防遗忘看门狗：正常工作时间之外连续工作超过 x 分钟 → 提醒窗；y 秒无操作 → 自动停止。

use chrono::{Local, NaiveTime};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 正在计时
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Running {
    pub project_id: i64,
    pub task_id: i64,
    pub started_at: String,
    /// 开始时刻（毫秒时间戳）
    pub started_ms: i64,
}

/// 停止后待确认的阶段
#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// 提醒窗是否已弹出
    pub reminder_active: Mutex<bool>,
    /// 提醒窗自动停止的时刻（毫秒）
    pub remind_deadline_ms: Mutex<i64>,
    /// 最近一次「仍在工作」/ 进入正常工作时间 / 开始计时的时刻
    pub ack_ms: Mutex<i64>,
    /// 计时「代次」：停止后旧的心跳线程据此退出，避免快速重启时出现两条线程
    pub generation: Mutex<u64>,
}

impl TimerState {
    fn clear_reminder(&self) {
        if let Ok(mut a) = self.reminder_active.lock() {
            *a = false;
        }
        if let Ok(mut d) = self.remind_deadline_ms.lock() {
            *d = 0;
        }
    }
}

fn now_ms() -> i64 {
    Local::now().timestamp_millis()
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PersistedTimer {
    running: Option<Running>,
    pending: Option<PendingStage>,
}

fn state_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(crate::config::config_dir(app)?.join("timer-state.json"))
}

/// 把运行态 / 待保存阶段写入磁盘，应用意外退出或重启后可恢复
fn persist(app: &AppHandle) {
    let state = app.state::<TimerState>();
    let running = state.running.lock().ok().and_then(|g| g.clone());
    let pending = state.pending.lock().ok().and_then(|g| g.clone());
    let snap = PersistedTimer { running, pending };
    let Ok(path) = state_file(app) else { return };
    let Ok(text) = serde_json::to_string_pretty(&snap) else { return };
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, text).is_ok() {
        // Unix 可直接覆盖；Windows 的 rename 不能覆盖已有文件，失败时退回 copy
        if fs::rename(&tmp, &path).is_err() {
            let _ = fs::copy(&tmp, &path);
            let _ = fs::remove_file(&tmp);
        }
    }
}

/// 启动应用时恢复上次的计时状态；若在计时中，重启心跳线程
pub fn restore(app: &AppHandle) {
    let Ok(path) = state_file(app) else { return };
    let Ok(text) = fs::read_to_string(path) else { return };
    let Ok(snap) = serde_json::from_str::<PersistedTimer>(&text) else { return };
    let state = app.state::<TimerState>();
    if let Ok(mut g) = state.running.lock() {
        *g = snap.running;
    }
    if let Ok(mut p) = state.pending.lock() {
        *p = snap.pending;
    }
    let running = state.running.lock().map(|g| g.is_some()).unwrap_or(false);
    if running {
        if let Ok(mut a) = state.ack_ms.lock() {
            *a = now_ms();
        }
        let my_gen = {
            let mut g = match state.generation.lock() {
                Ok(g) => g,
                Err(_) => return,
            };
            *g += 1;
            *g
        };
        spawn_ticker(app, my_gen);
        let _ = app.emit("timer-restored", ());
    }
}

/// 开始计时
pub fn start(app: &AppHandle, project_id: i64, task_id: i64) -> Result<(), String> {
    let state = app.state::<TimerState>();
    {
        let pending = state.pending.lock().map_err(|_| "计时状态锁异常".to_string())?;
        if pending.is_some() {
            return Err("请先保存或删除上次阶段记录".into());
        }
    }
    {
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
    }
    state.clear_reminder();
    if let Ok(mut a) = state.ack_ms.lock() {
        *a = now_ms();
    }
    let my_gen = {
        let mut g = state.generation.lock().map_err(|_| "计时状态锁异常".to_string())?;
        *g += 1;
        *g
    };

    persist(app);
    spawn_ticker(app, my_gen);
    Ok(())
}

/// 每秒推一次心跳；顺带跑防遗忘看门狗
fn spawn_ticker(app: &AppHandle, my_gen: u64) {
    let handle = app.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(1));
        let st = handle.state::<TimerState>();
        if st.generation.lock().map(|g| *g != my_gen).unwrap_or(true) {
            break;
        }
        let running = match st.running.lock() {
            Ok(g) => g.clone(),
            Err(_) => break,
        };
        let Some(r) = running else { break };

        let now = Local::now();
        let elapsed = ((now.timestamp_millis() - r.started_ms) / 1000).max(0);
        let _ = handle.emit("timer-tick", elapsed);

        let cfg = crate::config::load(&handle);
        let start_time = NaiveTime::parse_from_str(&cfg.reminder.work_start, "%H:%M")
            .unwrap_or_else(|_| NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        let end_time = NaiveTime::parse_from_str(&cfg.reminder.work_end, "%H:%M")
            .unwrap_or_else(|_| NaiveTime::from_hms_opt(18, 0, 0).unwrap());
        let t = now.time();
        let in_normal_hours = if start_time <= end_time {
            t >= start_time && t < end_time
        } else {
            // 跨夜班：下班时间到上班时间之间算「非正常」
            !(t >= end_time && t < start_time)
        };

        let active = st.reminder_active.lock().map(|g| *g).unwrap_or(false);
        if in_normal_hours {
            if active {
                st.clear_reminder();
                let _ = handle.emit("reminder-hide", ());
            }
            if let Ok(mut a) = st.ack_ms.lock() {
                *a = now.timestamp_millis();
            }
            continue;
        }

        let x_ms = (cfg.reminder.limit_x_minutes.max(1) as i64) * 60_000;
        let y_sec = cfg.reminder.stop_y_seconds.max(5) as i64;
        let ack = st.ack_ms.lock().map(|g| *g).unwrap_or(0);
        if !active {
            if now.timestamp_millis() - ack >= x_ms {
                if let Ok(mut a) = st.reminder_active.lock() {
                    *a = true;
                }
                let deadline = now.timestamp_millis() + y_sec * 1000;
                if let Ok(mut d) = st.remind_deadline_ms.lock() {
                    *d = deadline;
                }
                let _ = handle.emit("reminder-show", serde_json::json!({ "seconds": y_sec }));
            }
        } else {
            let deadline = st.remind_deadline_ms.lock().map(|g| *g).unwrap_or(0);
            let remain = ((deadline - now.timestamp_millis()) / 1000).max(0);
            if remain <= 0 {
                // 无操作 → 自动停止，并生成待保存阶段
                match take_pending(&st) {
                    Ok(Some(pending)) => {
                        st.clear_reminder();
                        persist(&handle);
                        let _ = handle.emit("timer-auto-stopped", pending);
                        break;
                    }
                    Ok(None) => break,
                    Err(_) => break,
                }
            } else {
                let _ = handle.emit("reminder-tick", remain);
            }
        }
    });
}

/// 把当前 running 转成 pending（停止 / 自动停止共用）
fn take_pending(state: &TimerState) -> Result<Option<PendingStage>, String> {
    let running = {
        let mut g = state.running.lock().map_err(|_| "计时状态锁异常".to_string())?;
        g.take()
    };
    let Some(running) = running else { return Ok(None) };
    let ended = Local::now();
    let pending = PendingStage {
        project_id: running.project_id,
        task_id: running.task_id,
        started_at: running.started_at.clone(),
        ended_at: ended.format("%Y-%m-%d %H:%M:%S").to_string(),
        duration: ((ended.timestamp_millis() - running.started_ms) / 1000).max(0),
    };
    {
        let mut p = state.pending.lock().map_err(|_| "计时状态锁异常".to_string())?;
        *p = Some(pending.clone());
    }
    Ok(Some(pending))
}

/// 停止计时 → 返回待保存的阶段（不落库）
pub fn stop(app: &AppHandle) -> Result<PendingStage, String> {
    let state = app.state::<TimerState>();
    let pending = take_pending(&state)?.ok_or_else(|| "当前没有在计时".to_string())?;
    if let Ok(mut g) = state.generation.lock() {
        *g += 1;
    }
    state.clear_reminder();
    persist(app);
    let _ = app.emit("reminder-hide", ());
    Ok(pending)
}

/// 丢弃待保存阶段（F3 点「删除记录」）
pub fn discard(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<TimerState>();
    let mut p = state.pending.lock().map_err(|_| "计时状态锁异常".to_string())?;
    *p = None;
    drop(p);
    persist(app);
    Ok(())
}

/// 用户在提醒窗点「仍在工作」：重置提醒计时
pub fn keep_working(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<TimerState>();
    state.clear_reminder();
    if let Ok(mut a) = state.ack_ms.lock() {
        *a = now_ms();
    }
    let _ = app.emit("reminder-hide", ());
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
    pub pending: Option<PendingStage>,
    pub remind_active: bool,
    pub remind_remaining: i64,
}

pub fn info(app: &AppHandle) -> TimerInfo {
    let state = app.state::<TimerState>();
    let running = state.running.lock().ok().and_then(|g| g.clone());
    let pending = state.pending.lock().ok().and_then(|g| g.clone());
    let active = state.reminder_active.lock().map(|g| *g).unwrap_or(false);
    let deadline = state.remind_deadline_ms.lock().map(|g| *g).unwrap_or(0);
    let elapsed = running
        .as_ref()
        .map(|r| ((Local::now().timestamp_millis() - r.started_ms) / 1000).max(0))
        .unwrap_or(0);
    TimerInfo {
        running: running.is_some(),
        elapsed,
        project_id: running.as_ref().map(|r| r.project_id),
        task_id: running.as_ref().map(|r| r.task_id),
        has_pending: pending.is_some(),
        pending,
        remind_active: active,
        remind_remaining: if active {
            ((deadline - now_ms()) / 1000).max(0)
        } else {
            0
        },
    }
}
