//! 报告：按 `report-template.yaml` 渲染初稿，并可选调用 OpenAI 兼容接口按 Skill 改写。
//!
//! 模板只有三块：字段对照 / 自定义文字 / 报告正文（日报 · 周报 · 月报）。
//! 解析失败一律回退编译进二进制的默认模板，报告不中断。

use crate::config;
use crate::db::Db;
use crate::stats::{self, fmt_duration, PeriodInfo, ReportRecord};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};
use tauri::AppHandle;

const DEFAULT_TEMPLATE: &str = include_str!("../../resources/report-template.yaml");
const DEFAULT_SKILL: &str = include_str!("../../resources/report-skill.md");

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Template {
    #[serde(rename = "字段对照")]
    pub fields: BTreeMap<String, String>,
    #[serde(rename = "自定义文字")]
    pub custom: BTreeMap<String, String>,
    #[serde(rename = "日报")]
    pub daily: String,
    #[serde(rename = "周报")]
    pub weekly: String,
    #[serde(rename = "月报")]
    pub monthly: String,
}

impl Template {
    fn document(&self, doc_kind: &str) -> &str {
        match doc_kind {
            "周报" => &self.weekly,
            "月报" => &self.monthly,
            _ => &self.daily,
        }
    }
    fn valid(&self) -> bool {
        !self.fields.is_empty() && !self.daily.trim().is_empty()
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReportResult {
    pub text: String,
    /// 模板文件缺失 / 解析失败时为 true
    pub used_fallback: bool,
    pub template_path: String,
    pub skill_path: String,
    pub period: PeriodInfo,
}

fn parse_template(text: &str) -> Option<Template> {
    serde_yaml::from_str::<Template>(text).ok().filter(|t| t.valid())
}

fn load_template(app: &AppHandle) -> (Template, bool) {
    let path = config::report_file(app, "template").ok();
    if let Some(path) = path {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(t) = parse_template(&text) {
                return (t, false);
            }
        }
    }
    (
        parse_template(DEFAULT_TEMPLATE).unwrap_or_default(),
        true,
    )
}

/* ---------------- 模板渲染 ---------------- */

fn source_of(tpl: &Template, alias: &str) -> String {
    tpl.fields
        .get(alias)
        .cloned()
        .unwrap_or_else(|| alias.to_string())
}

/// 一条记录的字段值；没有的字段返回 None
fn record_value(rec: &ReportRecord, source: &str) -> Option<String> {
    let v = match source {
        "project_name" => rec.project_name.clone(),
        "task_name" => rec.task_name.clone(),
        "result" => {
            if rec.result.trim().is_empty() {
                "（未填写成果）".into()
            } else {
                rec.result.clone()
            }
        }
        "next_goal" => {
            if rec.next_goal.trim().is_empty() {
                "（未填写计划）".into()
            } else {
                rec.next_goal.clone()
            }
        }
        "duration" => fmt_duration(rec.duration),
        "priority" => rec.priority.clone(),
        "due_date" => rec.due_date.clone(),
        "stage_no" => rec.stage_id.to_string(),
        "date" | "period_date" => rec.ended_at.date().format("%Y-%m-%d").to_string(),
        "started_at" => rec.started_at.format("%Y-%m-%d %H:%M").to_string(),
        "ended_at" => rec.ended_at.format("%Y-%m-%d %H:%M").to_string(),
        _ => return None,
    };
    Some(v)
}

/// 周期级字段；没有的字段返回 None
fn context_value(ctx: &BTreeMap<String, String>, source: &str) -> Option<String> {
    if source.starts_with("agg_") || source.starts_with("period_") {
        return ctx.get(source).cloned();
    }
    ctx.get(source).cloned()
}

fn is_record_source(source: &str) -> bool {
    matches!(
        source,
        "project_name"
            | "task_name"
            | "result"
            | "next_goal"
            | "duration"
            | "priority"
            | "due_date"
            | "stage_no"
            | "started_at"
            | "ended_at"
            | "date"
    )
}

/// 替换一行里的占位符；`rec` 为 Some 时，任一引用为空 → 整行不输出
fn fill_pattern(
    pattern: &str,
    rec: Option<&ReportRecord>,
    tpl: &Template,
    ctx: &BTreeMap<String, String>,
) -> String {
    let mut ok = true;
    let out = replace_placeholders(pattern, |alias| {
        let source = source_of(tpl, alias);
        let value = if let Some(rec) = rec {
            if is_record_source(&source) {
                record_value(rec, &source)
            } else {
                context_value(ctx, &source)
            }
        } else {
            context_value(ctx, &source)
        };
        let value = value.unwrap_or_default();
        if value.trim().is_empty() {
            ok = false;
        }
        value
    });
    if ok {
        out
    } else {
        String::new()
    }
}

fn replace_placeholders(
    text: &str,
    mut resolve: impl FnMut(&str) -> String,
) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            let key = &after[..end];
            if key.trim().is_empty() || key.contains('{') {
                out.push('{');
                rest = after;
                continue;
            }
            out.push_str(&resolve(key.trim()));
            rest = &after[end + 1..];
        } else {
            out.push_str(&rest[start..]);
            return out;
        }
    }
    out.push_str(rest);
    out
}

/// 自定义文字：每条记录渲染一次，空行丢弃、重复行去掉
fn render_custom(
    tpl: &Template,
    key: &str,
    recs: &[ReportRecord],
    ctx: &BTreeMap<String, String>,
) -> Vec<String> {
    let Some(pattern) = tpl.custom.get(key) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut lines = Vec::new();
    for rec in recs {
        let line = fill_pattern(pattern, Some(rec), tpl, ctx);
        let line = line.trim();
        if !line.is_empty() && seen.insert(line.to_string()) {
            lines.push(line.to_string());
        }
    }
    lines
}

fn render_body(
    tpl: &Template,
    body: &str,
    recs: &[ReportRecord],
    ctx: &BTreeMap<String, String>,
) -> String {
    let mut out: Vec<String> = Vec::new();
    for line in body.split('\n') {
        // 1) 先展开「自定义文字」：整行就是一个占位符时，直接展开成多行
        let trimmed = line.trim();
        if trimmed.starts_with('{') && trimmed.ends_with('}') && !trimmed[1..trimmed.len() - 1].contains('}') {
            let key = trimmed[1..trimmed.len() - 1].trim();
            if tpl.custom.contains_key(key) {
                out.extend(render_custom(tpl, key, recs, ctx));
                continue;
            }
        }

        // 2) 行内引用自定义文字
        let mut segments: Vec<String> = vec![line.to_string()];
        for key in tpl.custom.keys() {
            let token = format!("{{{key}}}");
            if !line.contains(&token) {
                continue;
            }
            let block = render_custom(tpl, key, recs, ctx).join("\n");
            let mut next = Vec::new();
            for seg in segments {
                for piece in seg.replace(&token, &block).split('\n') {
                    next.push(piece.to_string());
                }
            }
            segments = next;
        }

        // 3) 展开剩余的记录级 / 周期级占位符
        for seg in segments {
            let sources: Vec<String> = placeholders(&seg)
                .into_iter()
                .map(|k| source_of(tpl, &k))
                .collect();
            let has_record = sources.iter().any(|s| is_record_source(s));
            if has_record {
                let mut seen = HashSet::new();
                for rec in recs {
                    let filled = fill_pattern(&seg, Some(rec), tpl, ctx);
                    let filled = filled.trim_end();
                    if !filled.is_empty() && seen.insert(filled.to_string()) {
                        out.push(filled.to_string());
                    }
                }
            } else {
                out.push(fill_pattern(&seg, None, tpl, ctx));
            }
        }
    }
    // 去掉首尾多余空行，正文内部的空行原样保留
    while out.first().is_some_and(|l| l.trim().is_empty()) {
        out.remove(0);
    }
    while out.last().is_some_and(|l| l.trim().is_empty()) {
        out.pop();
    }
    out.join("\n")
}

fn placeholders(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('}') {
            let key = after[..end].trim();
            if !key.is_empty() {
                keys.push(key.to_string());
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    keys
}

/// 渲染报告初稿
pub fn generate(app: &AppHandle, db: &Db, key: &str) -> Result<ReportResult, String> {
    let period = stats::resolve_period(key);
    let recs = stats::records(db, &period)?;
    let (tpl, used_fallback) = load_template(app);
    let text = render_report(&tpl, &period, &recs);
    Ok(ReportResult {
        text,
        used_fallback,
        template_path: config::report_file(app, "template")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        skill_path: config::report_file(app, "skill")
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default(),
        period,
    })
}

/// 不依赖 Tauri 的纯渲染（便于测试）
pub fn render_report(tpl: &Template, period: &PeriodInfo, recs: &[ReportRecord]) -> String {
    let total: i64 = recs.iter().map(|r| r.duration).sum();
    let projects: HashSet<i64> = recs.iter().map(|r| r.project_id).collect();
    let tasks: HashSet<(i64, i64)> = recs.iter().map(|r| (r.project_id, r.task_id)).collect();
    let mut ctx: BTreeMap<String, String> = BTreeMap::new();
    ctx.insert("period_date".into(), period.label.clone());
    ctx.insert("period_label".into(), period.label.clone());
    ctx.insert("period_start".into(), period.start_date.clone());
    ctx.insert("period_end".into(), period.end_date.clone());
    ctx.insert("period_key".into(), period.key.clone());
    ctx.insert("agg_total_duration".into(), fmt_duration(total));
    ctx.insert("agg_project_count".into(), projects.len().to_string());
    ctx.insert("agg_task_count".into(), tasks.len().to_string());
    ctx.insert("agg_record_count".into(), recs.len().to_string());

    let body = tpl.document(&period.doc_kind);
    if body.trim().is_empty() {
        return String::new();
    }
    render_body(tpl, body, recs, &ctx)
}

/* ---------------- AI 改写 ---------------- */

#[derive(Serialize)]
struct ChatMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
}

fn skill_text(app: &AppHandle) -> String {
    config::report_file(app, "skill")
        .ok()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_else(|| DEFAULT_SKILL.to_string())
}

/// 按 Skill 文件调用 OpenAI 兼容接口
pub async fn ai_rewrite(
    app: &AppHandle,
    db: &Db,
    key: &str,
    draft: &str,
) -> Result<String, String> {
    let cfg = config::load(app);
    let api_key = cfg.llm.api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("未配置 API Key，已保留模板报告".into());
    }
    let period = stats::resolve_period(key);
    let recs = stats::records(db, &period)?;
    let structured = serde_json::json!({
        "period": period,
        "records": recs.iter().map(|r| serde_json::json!({
            "projectName": r.project_name,
            "taskName": r.task_name,
            "result": r.result,
            "nextGoal": r.next_goal,
            "durationSeconds": r.duration,
            "durationText": fmt_duration(r.duration),
            "priority": r.priority,
            "dueDate": r.due_date,
            "startedAt": r.started_at.format("%Y-%m-%d %H:%M:%S").to_string(),
            "endedAt": r.ended_at.format("%Y-%m-%d %H:%M:%S").to_string(),
        })).collect::<Vec<_>>()
    });

    let user = format!(
        "【周期】\n{}\n\n【模板初稿】\n{}\n\n【结构化数据】\n{}\n\n请严格按 Skill 的约束与格式输出改写后的报告正文，不要输出任何解释。",
        period.label,
        draft,
        serde_json::to_string_pretty(&structured).unwrap_or_default()
    );
    let req = ChatRequest {
        model: if cfg.llm.model.trim().is_empty() {
            "gpt-4o-mini".into()
        } else {
            cfg.llm.model.trim().to_string()
        },
        messages: vec![
            ChatMessage {
                role: "system",
                content: skill_text(app),
            },
            ChatMessage {
                role: "user",
                content: user,
            },
        ],
        temperature: 0.3,
    };

    let base = cfg.llm.base_url.trim().trim_end_matches('/');
    let url = if base.ends_with("/chat/completions") {
        base.to_string()
    } else {
        format!("{base}/chat/completions")
    };
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(90))
        .build()
        .map_err(|e| format!("初始化网络客户端失败：{e}"))?;
    let resp = client
        .post(&url)
        .bearer_auth(&api_key)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("调用 LLM 失败：{e}"))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("读取 LLM 响应失败：{e}"))?;
    if !status.is_success() {
        let brief: String = body.chars().take(300).collect();
        return Err(format!("LLM 返回 {status}：{brief}"));
    }
    let json: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("LLM 响应解析失败：{e}"))?;
    let content = json
        .get("choices")
        .and_then(|v| v.get(0))
        .and_then(|v| v.get("message"))
        .and_then(|v| v.get("content"))
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "LLM 未返回有效文本".to_string())?;
    Ok(content.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn rec(project: &str, task: &str, result: &str, next: &str) -> ReportRecord {
        let d = NaiveDate::from_ymd_opt(2026, 10, 20)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        ReportRecord {
            project_id: 1,
            task_id: 1,
            project_name: project.into(),
            task_name: task.into(),
            stage_id: 1,
            started_at: d,
            ended_at: d + chrono::Duration::hours(2),
            duration: 7200,
            result: result.into(),
            next_goal: next.into(),
            priority: "中".into(),
            due_date: String::new(),
        }
    }

    #[test]
    fn render_default_style() {
        let tpl = parse_template(DEFAULT_TEMPLATE).expect("default template");
        let period = stats::resolve_period("当日");
        let recs = vec![
            rec("项目A", "任务1", "完成接口", "联调"),
            rec("项目A", "任务2", "写文档", ""),
        ];
        let text = render_report(&tpl, &period, &recs);
        assert!(text.contains("【工作成果】"), "{text}");
        let (result_part, plan_part) = text.split_once("【工作计划】").expect("two sections");
        assert!(result_part.contains("项目A · 任务1：完成接口"), "{text}");
        assert!(result_part.contains("项目A · 任务2：写文档"), "成果非空就应输出：{text}");
        assert!(plan_part.contains("项目A · 任务1：联调"), "{text}");
        assert!(plan_part.contains("项目A · 任务2：（未填写计划）"), "计划为空也要输出，不能漏任务：{text}");
    }

    #[test]
    fn empty_result_uses_placeholder() {
        let tpl = parse_template(DEFAULT_TEMPLATE).unwrap();
        let period = stats::resolve_period("当日");
        let recs = vec![rec("P", "T", "", "下一步")];
        let text = render_report(&tpl, &period, &recs);
        let (result_part, plan_part) = text.split_once("【工作计划】").expect("two sections");
        // 看板里能看到的任务，报告里也要出现，不能因为成果为空整条消失
        assert!(result_part.contains("P · T：（未填写成果）"), "{text}");
        assert!(plan_part.contains("P · T：下一步"), "{text}");
    }
}
