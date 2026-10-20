//! 用户配置（`config.toml`）
//!
//! 首次启动生成默认文件；缺失的项用默认值补齐；改坏了也不至于让应用起不来。

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub llm: Llm,
    pub appearance: Appearance,
    pub reminder: Reminder,
    pub report: Report,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Llm {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Appearance {
    /// dark | light | auto
    pub theme: String,
    /// 根字号（px），界面字号统一走 rem
    pub font_size: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Reminder {
    pub work_start: String,
    pub work_end: String,
    pub limit_x_minutes: u32,
    pub stop_y_seconds: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default, rename_all = "camelCase")]
pub struct Report {
    pub template_file: String,
    pub skill_file: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            llm: Llm::default(),
            appearance: Appearance::default(),
            reminder: Reminder::default(),
            report: Report::default(),
        }
    }
}
impl Default for Llm {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".into(),
            api_key: String::new(),
            model: "gpt-4o-mini".into(),
        }
    }
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            font_size: 16,
        }
    }
}
impl Default for Reminder {
    fn default() -> Self {
        Self {
            work_start: "09:00".into(),
            work_end: "18:00".into(),
            limit_x_minutes: 60,
            stop_y_seconds: 60,
        }
    }
}
impl Default for Report {
    fn default() -> Self {
        Self {
            template_file: "config/report-template.yaml".into(),
            skill_file: "config/report-skill.md".into(),
        }
    }
}

/// 配置目录（应用配置根目录）
pub fn config_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("取不到配置目录：{e}"))?;
    fs::create_dir_all(&dir).map_err(|e| format!("建配置目录失败：{e}"))?;
    Ok(dir)
}

fn config_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(config_dir(app)?.join("config.toml"))
}

/// 首次启动：写默认配置，并把随包的报告配置复制到 `config/`
pub fn init(app: &AppHandle) -> Result<(), String> {
    let dir = config_dir(app)?;
    let file = dir.join("config.toml");
    if !file.exists() {
        let cfg = Config::default();
        fs::write(&file, toml::to_string_pretty(&cfg).map_err(|e| e.to_string())?)
            .map_err(|e| format!("写配置失败：{e}"))?;
    }
    // 报告的模板 / Skill：随包资源 → 配置目录
    let cfg_sub = dir.join("config");
    fs::create_dir_all(&cfg_sub).map_err(|e| format!("建 config 目录失败：{e}"))?;
    for name in ["report-template.yaml", "report-skill.md"] {
        let target = cfg_sub.join(name);
        if !target.exists() {
            if let Some(res) = app.path().resolve(format!("resources/{name}"), tauri::path::BaseDirectory::Resource) {
                if res.exists() {
                    let _ = fs::copy(&res, &target);
                }
            }
        }
    }
    Ok(())
}

/// 读取配置（容错：读失败就用默认值）
pub fn load(app: &AppHandle) -> Config {
    let Ok(file) = config_file(app) else {
        return Config::default();
    };
    let Ok(text) = fs::read_to_string(&file) else {
        return Config::default();
    };
    toml::from_str::<Config>(&text).unwrap_or_default()
}

/// 保存配置
pub fn save(app: &AppHandle, cfg: &Config) -> Result<(), String> {
    let file = config_file(app)?;
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    fs::write(&file, text).map_err(|e| format!("写配置失败：{e}"))
}
