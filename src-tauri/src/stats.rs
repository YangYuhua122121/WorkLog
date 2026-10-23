//! 工时统计：周期计算、阶段明细读取、甘特 / 扇形所需的结构化聚合
//!
//! 周期全部按**本地自然时间**归组：周从周一开始，月从 1 日开始。

use crate::db::{with_conn, Db};
use chrono::{Datelike, Duration as ChronoDuration, Local, NaiveDate, NaiveDateTime, Timelike};
use serde::Serialize;
use std::collections::BTreeMap;

/// 一个统计周期
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodInfo {
    pub key: String,
    /// 展示用周期名（日报 / 周报 / 月报标题）
    pub label: String,
    /// 起始日（含）
    pub start_date: String,
    /// 结束日（含，便于展示）
    pub end_date: String,
    /// 日报 / 周报 / 月报
    pub doc_kind: String,
}

impl PeriodInfo {
    pub fn start(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.start_date, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive())
    }
    pub fn end(&self) -> NaiveDate {
        NaiveDate::parse_from_str(&self.end_date, "%Y-%m-%d").unwrap_or_else(|_| Local::now().date_naive())
    }
}

/// 解析周期关键字：当日 / 昨日 / 本周 / 本月 / 上周 / 上月
pub fn resolve_period(key: &str) -> PeriodInfo {
    let today = Local::now().date_naive();
    let monday = today - ChronoDuration::days(today.weekday().num_days_from_monday() as i64);
    let first_this_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap_or(today);
    let first_next_month = if today.month() == 12 {
        NaiveDate::from_ymd_opt(today.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(today.year(), today.month() + 1, 1)
    }
    .unwrap_or(today);
    let first_prev_month = if today.month() == 1 {
        NaiveDate::from_ymd_opt(today.year() - 1, 12, 1)
    } else {
        NaiveDate::from_ymd_opt(today.year(), today.month() - 1, 1)
    }
    .unwrap_or(today);

    let (start, end_ex, label, doc) = match key {
        "昨日" => (
            today - ChronoDuration::days(1),
            today,
            format!("{}", today - ChronoDuration::days(1)),
            "日报",
        ),
        "本周" => (
            monday,
            monday + ChronoDuration::days(7),
            format!("{} ~ {}", monday, monday + ChronoDuration::days(6)),
            "周报",
        ),
        "本月" => (
            first_this_month,
            first_next_month,
            format!("{}", first_this_month.format("%Y-%m")),
            "月报",
        ),
        "上周" => (
            monday - ChronoDuration::days(7),
            monday,
            format!(
                "{} ~ {}",
                monday - ChronoDuration::days(7),
                monday - ChronoDuration::days(1)
            ),
            "周报",
        ),
        "上月" => (
            first_prev_month,
            first_this_month,
            format!("{}", first_prev_month.format("%Y-%m")),
            "月报",
        ),
        _ => (today, today + ChronoDuration::days(1), format!("{today}"), "日报"),
    };

    PeriodInfo {
        key: if key.is_empty() { "当日".into() } else { key.to_string() },
        label,
        start_date: start.format("%Y-%m-%d").to_string(),
        end_date: (end_ex - ChronoDuration::days(1))
            .format("%Y-%m-%d")
            .to_string(),
        doc_kind: doc.to_string(),
    }
}

/// 报告 / 看板共用的阶段记录
#[derive(Clone, Debug)]
pub struct ReportRecord {
    pub project_id: i64,
    pub task_id: i64,
    pub project_name: String,
    pub task_name: String,
    pub stage_id: i64,
    pub started_at: NaiveDateTime,
    pub ended_at: NaiveDateTime,
    pub duration: i64,
    pub result: String,
    pub next_goal: String,
    pub priority: String,
    pub due_date: String,
}

/// 读取周期内的阶段明细（按结束时间归属周期）
pub fn records(db: &Db, period: &PeriodInfo) -> Result<Vec<ReportRecord>, String> {
    with_conn(db, |conn| records_from_conn(conn, period))
}

fn records_from_conn(
    conn: &rusqlite::Connection,
    period: &PeriodInfo,
) -> Result<Vec<ReportRecord>, String> {
    let start = format!("{} 00:00:00", period.start_date);
    // end_date 是「含」的结束日；SQL 用左闭右开，所以加一天
    let end_date = NaiveDate::parse_from_str(&period.end_date, "%Y-%m-%d")
        .unwrap_or_else(|_| period.start())
        + ChronoDuration::days(1);
    let end = format!("{} 00:00:00", end_date.format("%Y-%m-%d"));
    {
        let mut stmt = conn
            .prepare(
                "SELECT p.project_id, p.task_id, p.project_name, COALESCE(p.task_name,''),
                        s.stage_id, s.started_at, s.ended_at, COALESCE(s.duration, 0),
                        COALESCE(s.result,''), COALESCE(s.next_goal,''),
                        p.priority, COALESCE(p.due_date,'')
                 FROM stage s
                 JOIN project_task p ON p.project_id = s.project_id AND p.task_id = s.task_id
                 WHERE s.ended_at IS NOT NULL AND s.ended_at >= ?1 AND s.ended_at < ?2
                 ORDER BY s.ended_at ASC, s.project_id ASC, s.task_id ASC, s.stage_id ASC",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(rusqlite::params![start, end], |r| {
                let started_s: String = r.get(5)?;
                let ended_s: String = r.get(6)?;
                let started = parse_dt(&started_s);
                let ended = parse_dt(&ended_s);
                let stored: i64 = r.get(7)?;
                let duration = if stored > 0 {
                    stored
                } else {
                    ended
                        .as_ref()
                        .zip(started.as_ref())
                        .map(|(a, b)| (*a - *b).num_seconds().max(0))
                        .unwrap_or(0)
                };
                Ok(ReportRecord {
                    project_id: r.get(0)?,
                    task_id: r.get(1)?,
                    project_name: r.get(2)?,
                    task_name: r.get(3)?,
                    stage_id: r.get(4)?,
                    started_at: started.unwrap_or_else(|| ended.unwrap_or_default()),
                    ended_at: ended.unwrap_or_default(),
                    duration,
                    result: r.get(8)?,
                    next_goal: r.get(9)?,
                    priority: r.get(10)?,
                    due_date: r.get(11)?,
                })
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        Ok(rows)
    }
}

fn parse_dt(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
        .ok()
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok())
        .or_else(|| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").ok())
}

/* ---------------- 看板结构 ---------------- */

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Axis {
    /// 时 / 日
    pub unit: String,
    pub min: f64,
    pub span: f64,
    pub ticks: Vec<f64>,
    pub labels: Vec<String>,
    /// 逐格网格线（比刻度密）
    pub grid: Vec<f64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttTask {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub duration: i64,
    pub duration_text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GanttRow {
    pub project_name: String,
    pub color_index: usize,
    pub tasks: Vec<GanttTask>,
    /// 周 / 月视图：按天聚合的该项目任务清单（甘特条只画到天，不再重叠任务名）
    pub day_tasks: Vec<DayTasks>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTask {
    pub name: String,
    pub duration: i64,
    pub duration_text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayTasks {
    /// 横轴位置：第几天（0 基），end = start + 1
    pub start: f64,
    pub end: f64,
    pub date: String,
    pub duration: i64,
    pub duration_text: String,
    pub tasks: Vec<DayTask>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieTask {
    pub name: String,
    pub duration: i64,
    pub pct: f64,
    pub duration_text: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PieRow {
    pub project_name: String,
    pub color_index: usize,
    pub duration: i64,
    pub pct: f64,
    pub duration_text: String,
    pub tasks: Vec<PieTask>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub period: PeriodInfo,
    pub axis: Axis,
    pub rows: Vec<GanttRow>,
    pub pie: Vec<PieRow>,
    pub total_duration: i64,
    pub total_duration_text: String,
    pub project_count: usize,
    pub task_count: usize,
    pub record_count: usize,
}

#[derive(Clone, Debug)]
struct Agg {
    project_id: i64,
    project_name: String,
    task_name: String,
    start: f64,
    end: f64,
    duration: i64,
    order: NaiveDateTime,
}

#[derive(Default)]
struct DayAgg {
    duration: i64,
    tasks: BTreeMap<i64, (String, i64)>,
}

/// 把一条阶段记录按自然日拆开（跨天计时按重叠秒数分摊）
fn split_record_by_day(r: &ReportRecord, period: &PeriodInfo) -> Vec<(i64, i64)> {
    let period_start = period.start().and_hms_opt(0, 0, 0);
    let period_end = (period.end() + ChronoDuration::days(1)).and_hms_opt(0, 0, 0);
    let (Some(ps), Some(pe)) = (period_start, period_end) else {
        return vec![];
    };
    let s = r.started_at.max(ps);
    let e = r.ended_at.min(pe);
    if e <= s {
        return vec![];
    }
    let total = (r.ended_at - r.started_at).num_seconds().max(1) as f64;
    let mut out = Vec::new();
    let mut day = s.date();
    while day < e.date() || (day == e.date() && e.time() > chrono::NaiveTime::MIN) {
        let ds = day.and_hms_opt(0, 0, 0).unwrap();
        let de = ds + ChronoDuration::days(1);
        let overlap = (e.min(de) - s.max(ds)).num_seconds().max(0);
        if overlap > 0 {
            let offset = day.signed_duration_since(period.start()).num_days();
            let share = (r.duration as f64 * overlap as f64 / total).round() as i64;
            if share > 0 {
                out.push((offset, share));
            }
        }
        day += ChronoDuration::days(1);
    }
    out
}

/// 按周期聚合甘特 / 扇形数据
pub fn summary(db: &Db, key: &str) -> Result<Summary, String> {
    let period = resolve_period(key);
    let recs = records(db, &period)?;
    build_summary(&period, &recs)
}

fn build_summary(period: &PeriodInfo, recs: &[ReportRecord]) -> Result<Summary, String> {
    let daily = period.doc_kind == "日报";
    let start_date = period.start();
    let unit = if daily { "时" } else { "日" };

    // 每行一个项目-任务；同一任务的多段计时合并成一条（取最早开始 / 最晚结束，时长累加）
    let mut by_task: BTreeMap<(i64, i64), Agg> = BTreeMap::new();
    for r in recs {
        let (s, e) = axis_range(r, &start_date, daily);
        let entry = by_task.entry((r.project_id, r.task_id)).or_insert_with(|| Agg {
            project_id: r.project_id,
            project_name: r.project_name.clone(),
            task_name: r.task_name.clone(),
            start: s,
            end: e,
            duration: 0,
            order: r.started_at,
        });
        entry.start = entry.start.min(s);
        entry.end = entry.end.max(e);
        entry.duration += r.duration;
        if r.started_at < entry.order {
            entry.order = r.started_at;
        }
    }

    // 甘特横轴
    let (mut axis_min, mut axis_span) = if daily {
        if recs.is_empty() {
            (0.0, 24.0)
        } else {
            let lo = recs
                .iter()
                .map(|r| axis_range(r, &start_date, true).0)
                .fold(f64::INFINITY, f64::min);
            let hi = recs
                .iter()
                .map(|r| axis_range(r, &start_date, true).1)
                .fold(f64::NEG_INFINITY, f64::max);
            let lo = lo.max(0.0).floor();
            let hi = hi.min(24.0).max(lo + 0.5).ceil();
            (lo, hi - lo)
        }
    } else {
        (0.0, period.end().signed_duration_since(start_date).num_days() as f64 + 1.0)
    };
    if axis_span < 0.5 {
        axis_span = 0.5;
    }
    if !daily {
        axis_min = 0.0;
    }

    let (ticks, labels, grid) = if daily {
        daily_axis(axis_min, axis_span)
    } else {
        day_axis(axis_min, axis_span, &period.key)
    };
    let axis = Axis {
        unit: unit.into(),
        min: axis_min,
        span: axis_span,
        ticks,
        labels,
        grid,
    };

    // 项目顺序：最早开始时间
    let mut projects: BTreeMap<i64, (String, Vec<Agg>, NaiveDateTime)> = BTreeMap::new();
    for agg in by_task.into_values() {
        let item = projects
            .entry(agg.project_id)
            .or_insert_with(|| (agg.project_name.clone(), Vec::new(), agg.order));
        item.0 = agg.project_name.clone();
        if agg.order < item.2 {
            item.2 = agg.order;
        }
        item.1.push(agg);
    }
    let mut project_list: Vec<_> = projects.into_values().collect();
    project_list.sort_by(|a, b| a.2.cmp(&b.2).then_with(|| a.0.cmp(&b.0)));

    // 周 / 月：项目 × 天 → 当天各任务时长，供悬浮浮窗展示
    let mut day_map: BTreeMap<(i64, i64), DayAgg> = BTreeMap::new();
    if !daily {
        for r in recs {
            for (offset, share) in split_record_by_day(r, period) {
                let entry = day_map.entry((r.project_id, offset)).or_default();
                entry.duration += share;
                let name = if r.task_name.is_empty() {
                    "（未命名任务）".to_string()
                } else {
                    r.task_name.clone()
                };
                let t = entry.tasks.entry(r.task_id).or_insert((name, 0));
                t.1 += share;
            }
        }
    }

    let mut rows = Vec::new();
    let mut pie = Vec::new();
    let total_duration: i64 = recs.iter().map(|r| r.duration).sum();
    let mut project_count = 0usize;
    for (idx, (proj_name, mut aggs, _)) in project_list.into_iter().enumerate() {
        project_count += 1;
        let project_id = aggs.first().map(|a| a.project_id).unwrap_or(0);
        aggs.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        let color_index = idx % 6 + 1;

        let mut day_tasks: Vec<DayTasks> = Vec::new();
        if !daily {
            for ((_, offset), agg) in day_map.iter().filter(|((pid, _), _)| *pid == project_id) {
                let mut tasks: Vec<DayTask> = agg
                    .tasks
                    .values()
                    .map(|(name, sec)| DayTask {
                        name: name.clone(),
                        duration: *sec,
                        duration_text: fmt_duration(*sec),
                    })
                    .collect();
                tasks.sort_by(|a, b| b.duration.cmp(&a.duration).then_with(|| a.name.cmp(&b.name)));
                let date = (start_date + ChronoDuration::days(*offset))
                    .format("%m-%d")
                    .to_string();
                day_tasks.push(DayTasks {
                    start: *offset as f64,
                    end: *offset as f64 + 1.0,
                    date,
                    duration: agg.duration,
                    duration_text: fmt_duration(agg.duration),
                    tasks,
                });
            }
        }
        let tasks: Vec<GanttTask> = aggs
            .iter()
            .map(|a| {
                // 同一时刻开始 / 结束的窄条给一个最小可见宽度
                let end = if a.end - a.start < axis.span * 0.012 {
                    a.start + axis.span * 0.012
                } else {
                    a.end
                };
                GanttTask {
                    name: if a.task_name.is_empty() {
                        "（未命名任务）".into()
                    } else {
                        a.task_name.clone()
                    },
                    start: a.start,
                    end: end.min(axis.min + axis.span),
                    duration: a.duration,
                    duration_text: fmt_duration(a.duration),
                }
            })
            .collect();
        let proj_total: i64 = aggs.iter().map(|a| a.duration).sum();
        let pct = if total_duration > 0 {
            proj_total as f64 / total_duration as f64 * 100.0
        } else {
            0.0
        };
        let pie_tasks = aggs
            .iter()
            .map(|a| PieTask {
                name: if a.task_name.is_empty() {
                    "（未命名任务）".into()
                } else {
                    a.task_name.clone()
                },
                duration: a.duration,
                pct: if total_duration > 0 {
                    a.duration as f64 / total_duration as f64 * 100.0
                } else {
                    0.0
                },
                duration_text: fmt_duration(a.duration),
            })
            .collect();
        pie.push(PieRow {
            project_name: proj_name.clone(),
            color_index,
            duration: proj_total,
            pct,
            duration_text: fmt_duration(proj_total),
            tasks: pie_tasks,
        });
        rows.push(GanttRow {
            project_name: proj_name,
            color_index,
            tasks,
            day_tasks,
        });
    }

    let task_count: usize = rows.iter().map(|r| r.tasks.len()).sum();
    Ok(Summary {
        period: period.clone(),
        axis,
        rows,
        pie,
        total_duration,
        total_duration_text: fmt_duration(total_duration),
        project_count,
        task_count,
        record_count: recs.len(),
    })
}

/// 记录在横轴上的位置
fn axis_range(r: &ReportRecord, start_date: &NaiveDate, daily: bool) -> (f64, f64) {
    if daily {
        // 横轴单位是小时，但 start/end 保留到秒，任务条本身是分钟级位置
        let secs_s = r.started_at.num_seconds_from_midnight() as f64;
        let secs_e = r.ended_at.num_seconds_from_midnight() as f64;
        let mut s = secs_s / 3600.0;
        let mut e = secs_e / 3600.0;
        // 跨天计时按当天边界裁剪
        if r.started_at.date() < *start_date {
            s = 0.0;
        }
        if r.ended_at.date() > *start_date {
            e = 24.0;
        }
        if e <= s {
            e = (s + r.duration as f64 / 3600.0).min(24.0).max(s + 0.02);
        }
        (s, e)
    } else {
        // 周 / 月按「日」为颗粒度：一条记录占满它发生的那些天
        let day = |d: &NaiveDateTime| {
            d.date().signed_duration_since(*start_date).num_days() as f64
        };
        let mut s = day(&r.started_at);
        let mut e = day(&r.ended_at) + 1.0;
        if s < 0.0 {
            s = 0.0;
        }
        if e <= s {
            e = s + 1.0;
        }
        (s, e)
    }
}

fn daily_axis(min: f64, span: f64) -> (Vec<f64>, Vec<String>, Vec<f64>) {
    // 小时刻度；任务条自身仍按秒 / 分钟定位
    let step = (span / 4.0).ceil().max(1.0);
    let mut ticks = vec![min];
    let mut v = (min / step).ceil() * step;
    while v < min + span - 0.01 {
        if v > min + 0.01 {
            ticks.push(v);
        }
        v += step;
    }
    ticks.push(min + span);
    ticks.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    let labels = ticks
        .iter()
        .map(|v| {
            let h = v.floor() as i64;
            let m = ((v - h as f64) * 60.0).round() as i64;
            let m = m.min(59);
            format!("{:02}:{:02}", h.min(24), m)
        })
        .collect();
    let grid = (min.ceil() as i64..=(min + span).floor() as i64)
        .map(|h| h as f64)
        .collect();
    (ticks, labels, grid)
}

fn day_axis(min: f64, span: f64, key: &str) -> (Vec<f64>, Vec<String>, Vec<f64>) {
    let grid = (0..=(span.round() as i64)).map(|d| min + d as f64).collect();
    if key == "本周" || key == "上周" {
        let names = ["周一", "周二", "周三", "周四", "周五", "周六", "周日"];
        let ticks = (0..7).map(|i| min + i as f64 + 0.5).collect();
        let labels = names.iter().map(|s| s.to_string()).collect();
        return (ticks, labels, grid);
    }
    // 月：每 5 天一个标签，落在两条网格线之间
    let days = span.round() as i64;
    let step = 5i64;
    let mut ticks = Vec::new();
    let mut labels = Vec::new();
    let mut d = 0i64;
    while d < days {
        let center = min + d as f64 + 0.5;
        ticks.push(center);
        labels.push(format!("{}日", d + 1));
        d += step;
    }
    (ticks, labels, grid)
}

/// 秒 → 中文时长
pub fn fmt_duration(sec: i64) -> String {
    if sec <= 0 {
        return "0 秒".into();
    }
    let h = sec / 3600;
    let m = sec % 3600 / 60;
    let s = sec % 60;
    if h > 0 {
        if m > 0 {
            format!("{h} 小时 {m} 分钟")
        } else {
            format!("{h} 小时")
        }
    } else if m > 0 {
        if s > 0 {
            format!("{m} 分钟 {s} 秒")
        } else {
            format!("{m} 分钟")
        }
    } else {
        format!("{s} 秒")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn duration_format() {
        assert_eq!(fmt_duration(0), "0 秒");
        assert_eq!(fmt_duration(59), "59 秒");
        assert_eq!(fmt_duration(90), "1 分钟 30 秒");
        assert_eq!(fmt_duration(3 * 3600 + 30 * 60), "3 小时 30 分钟");
    }

    #[test]
    fn daily_summary_has_rows() {
        let period = resolve_period("当日");
        let today = period.start();
        let at = |h: u32, m: u32| {
            today
                .and_hms_opt(h, m, 0)
                .unwrap()
        };
        let recs = vec![
            ReportRecord {
                project_id: 1,
                task_id: 1,
                project_name: "P".into(),
                task_name: "T".into(),
                stage_id: 1,
                started_at: at(9, 0),
                ended_at: at(11, 30),
                duration: 9000,
                result: "x".into(),
                next_goal: "y".into(),
                priority: "高".into(),
                due_date: String::new(),
            },
            ReportRecord {
                project_id: 1,
                task_id: 1,
                project_name: "P".into(),
                task_name: "T".into(),
                stage_id: 2,
                started_at: at(13, 0),
                ended_at: at(14, 0),
                duration: 3600,
                result: "x".into(),
                next_goal: "y".into(),
                priority: "高".into(),
                due_date: String::new(),
            },
        ];
        let s = build_summary(&period, &recs).unwrap();
        assert_eq!(s.rows.len(), 1);
        assert_eq!(s.rows[0].tasks[0].start, 9.0);
        assert_eq!(s.rows[0].tasks[0].end, 14.0);
        assert_eq!(s.rows[0].tasks[0].duration, 12600);
        assert_eq!(s.total_duration, 12600);
        assert_eq!(s.project_count, 1);
    }


    #[test]
    fn week_summary_is_day_grained() {
        let period = resolve_period("本周");
        let start = period.start();
        let d1 = start.and_hms_opt(9, 0, 0).unwrap();
        let d2 = (start + ChronoDuration::days(2)).and_hms_opt(15, 0, 0).unwrap();
        let mut recs = vec![];
        for (i, (s, e)) in [(d1, d1 + ChronoDuration::hours(2)), (d2, d2 + ChronoDuration::hours(1))]
            .into_iter()
            .enumerate()
        {
            recs.push(ReportRecord {
                project_id: 1,
                task_id: 1,
                project_name: "P".into(),
                task_name: "T".into(),
                stage_id: i as i64 + 1,
                started_at: s,
                ended_at: e,
                duration: 7200,
                result: "x".into(),
                next_goal: "y".into(),
                priority: "中".into(),
                due_date: String::new(),
            });
        }
        let summary = build_summary(&period, &recs).unwrap();
        assert_eq!(summary.axis.span, 7.0);
        assert_eq!(summary.rows[0].tasks[0].start, 0.0);
        assert_eq!(summary.rows[0].tasks[0].end, 3.0);
        assert_eq!(summary.rows[0].day_tasks.len(), 2, "应拆成两天");
        assert_eq!(summary.rows[0].day_tasks[0].start, 0.0);
        assert_eq!(summary.rows[0].day_tasks[1].start, 2.0);
        assert_eq!(summary.rows[0].day_tasks[0].tasks.len(), 1);
    }


    #[test]
    fn records_query_spans_period() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE project_task (
              project_id INTEGER, task_id INTEGER, project_name TEXT, task_name TEXT,
              status TEXT, priority TEXT, due_date TEXT, created_at TEXT, updated_at TEXT,
              remark TEXT, custom_fields TEXT, PRIMARY KEY(project_id, task_id));
            CREATE TABLE stage (
              project_id INTEGER, task_id INTEGER, stage_id INTEGER, started_at TEXT,
              ended_at TEXT, duration INTEGER, result TEXT, next_goal TEXT, status TEXT,
              PRIMARY KEY(project_id, task_id, stage_id));
            "#,
        )
        .unwrap();
        conn.execute(
            "INSERT INTO project_task VALUES (1,1,'P','T','未完成','中',NULL,'x','x','','{}')",
            [],
        )
        .unwrap();
        let period = resolve_period("本周");
        let monday = period.start();
        for (i, day) in [monday, monday + ChronoDuration::days(2)].iter().enumerate() {
            let start = day.and_hms_opt(9, 0, 0).unwrap();
            let end = day.and_hms_opt(10, 0, 0).unwrap();
            conn.execute(
                "INSERT INTO stage VALUES (1,1,?1,?2,?3,3600,'r','n','未完成')",
                rusqlite::params![
                    i as i64 + 1,
                    start.format("%Y-%m-%d %H:%M:%S").to_string(),
                    end.format("%Y-%m-%d %H:%M:%S").to_string()
                ],
            )
            .unwrap();
        }
        let out = records_from_conn(&conn, &period).unwrap();
        assert_eq!(out.len(), 2, "本周应取到跨两天的两条阶段");
    }

    #[test]
    fn dates_smoke() {
        assert!(dt("2026-10-20 09:00:00") > dt("2026-10-20 08:00:00"));
    }
}
