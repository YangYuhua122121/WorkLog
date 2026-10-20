# workLog 技术方案

> 配套文档：`worklog-PRD文档.md`（v0.30）
> 本文件为 v0.30 PRD 对应的技术方案，聚焦「怎么落地」，不展开需求。

## 1. 技术选型

| 层       | 选型                                  | 理由                                                                 |
| -------- | ------------------------------------- | -------------------------------------------------------------------- |
| 应用框架  | **Tauri 2**（Rust + WebView）          | 体积小（~10MB 级）、跨 macOS/Windows；原生侧（Rust）便于系统级能力   |
| 前端      | 原生 HTML/CSS/JS（首期）→ 可演进 Vue 3 | 首版界面与交互直接，无构建；若后期复杂可在 Tauri 内引入 Vite + Vue    |
| 本地存储  | **SQLite**（rusqlite）                 | 单机、事务、零运维；Tauri 有成熟绑定                                |
| 常驻交互  | Tauri 透明置顶窗口 + 系统托盘          | 悬浮球 / 主面板；托盘提供退出与显示                                 |
| 图表      | ECharts（WebView 渲染），Rust 保存文件  | 甘特 / 扇形；图片由 ECharts `getDataURL` 取出，Rust 经系统保存对话框落盘 |
| 报告导出  | **Rust 侧生成**：`rust_xlsxwriter`（Excel）/ 原生 CSV / 文本 | 数据本就在 Rust + SQLite；走系统「保存」对话框写文件，不依赖浏览器 |
| LLM      | 用户自配 OpenAI 兼容接口               | 仅报告时联网，Base URL / Key / 模型名存本地配置                     |

> 备选：Electron（生态成熟但体积大、内存高）；纯原生双端（开发成本高、迭代慢）。当前需求以 Tauri 2 综合最优。

## 2. 总体架构

```
┌───────────────────────────────────────────────┐
│  WebView（前端：HTML/CSS/JS）                   │
│  悬浮球UI · 数据表 · 看板(ECharts) · 报告       │
└───────────────▲───────────────┬───────────────┘
       Tauri IPC（invoke / event）               │
┌───────────────┴───────────────▼───────────────┐
│  Rust 核心（commands）                          │
│  计时服务 · 提醒服务 · 仓库(SQLite) · LLM客户端 │
└───────────────┬───────────────────────────────┘
                ▼
        SQLite 文件（AppData 本地目录）
        （LLM 仅在报告生成时通过 https 外呼）
```

- 前端只负责呈现与交互；**计时、落库、LLM 调用放在 Rust 侧**，保证窗口隐藏 / 后台时仍准确。
- 计时状态变化通过 Tauri `emit` 事件推给前端刷新。

## 3. 目录结构（Tauri 2）

```
worklog/
├─ src/                        # 前端（WebView 静态资源）
│  ├─ index.html
│  ├─ app.js / styles.css
│  └─ components/
├─ src-tauri/
│  ├─ src/
│  │  ├─ main.rs               # 入口、窗口、托盘
│  │  ├─ commands.rs           # #[tauri::command] 暴露给前端
│  │  ├─ timer.rs              # 计时服务（总计时器/提醒计时器）
│  │  ├─ db/
│  │  │  ├─ mod.rs
│  │  │  ├─ schema.rs          # 建表/迁移
│  │  │  ├─ project.rs         # 项目-任务维度 CRUD
│  │  │  └─ stage.rs           # 阶段明细 CRUD / 聚合
│  │  ├─ stats.rs              # 日/周/月聚合查询
│  │  ├─ report.rs             # 模板渲染（report-template.yaml）
│  │  └─ llm.rs                # OpenAI 兼容 HTTP 客户端
│  ├─ migrations/              # 数据库迁移（可选 tauri-plugin-sql）
│  └─ tauri.conf.json
├─ resources/                  # 随包资源（首次启动复制到配置目录）
│  ├─ report-template.yaml     # 报告固定格式化模板配置
│  └─ report-skill.md          # 报告撰写 Skill
└─ package.json
```

> 仓库内另存一份同内容的 `配置/report-template.yaml`、`配置/report-skill.md`，便于评审与直接编辑；打包时以 `resources/` 为准。
>
> 用户配置目录：macOS `~/Library/Application Support/workLog/`、Windows `%APPDATA%\workLog\`，下辖 `worklog.db`、`config.toml`（API Key 等）、`config/`（上面两个报告配置文件）。

## 4. 数据模型（两张表）

> 回答 F4 提问「是否分两个数据表」：**是，推荐两张表**。维度字段只存一次，阶段明细随每次计时追加，避免冗余与不一致。

**4.1 项目-任务维度表 `project_task`**

```sql
CREATE TABLE project_task (
  project_id   INTEGER NOT NULL,          -- 项目号，从 1 自增（全局）
  task_id      INTEGER NOT NULL,          -- 任务号，项目内从 1 自增
  project_name TEXT    NOT NULL,
  task_name    TEXT,
  status       TEXT    NOT NULL DEFAULT '未完成' CHECK(status IN ('未完成','已完成')),
  priority     TEXT    NOT NULL DEFAULT '中'  CHECK(priority IN ('高','中','低')),
  due_date     DATE,                      -- 截止日期，可空
  created_at   DATETIME NOT NULL,
  updated_at   DATETIME NOT NULL,         -- 最近操作时间（默认排序）
  remark       TEXT,
  custom_fields TEXT,                     -- 自定义字段，JSON 存键值
  PRIMARY KEY (project_id, task_id)
);
```

**4.2 阶段明细表 `stage`**

```sql
CREATE TABLE stage (
  project_id  INTEGER NOT NULL,
  task_id     INTEGER NOT NULL,
  stage_id    INTEGER NOT NULL,           -- 阶段号，项目-任务内从 1 自增
  started_at  DATETIME NOT NULL,
  ended_at    DATETIME,
  duration    INTEGER,                    -- 秒数 = ended-started
  result      TEXT,                       -- 阶段成果
  next_goal   TEXT,                       -- 下一目标
  status      TEXT,                       -- 该阶段提交时的任务状态
  PRIMARY KEY (project_id, task_id, stage_id),
  FOREIGN KEY (project_id, task_id) REFERENCES project_task(project_id, task_id)
);
CREATE INDEX idx_stage_ended ON stage(ended_at);
```

**4.3 派生字段**

- `DDL`（距截止天数）：不落库，查询时由 `due_date - 今日` 计算（`due_date` 为空则 DDL 空）。
- `累计时长`：按 `SUM(duration)` 聚合；阶段详情区按 stage 行展示。
- 前端数据表（去重）：对 `project_task` 展示，每行对应一个项目-任务；阶段在 P5-2 阶段列表中展开。

**4.4 编号规则**

- `project_id`：全局自增（`SELECT COALESCE(MAX(project_id),0)+1`）。
- `task_id`：项目内自增（`WHERE project_id=?` 取 MAX+1）。
- `stage_id`：项目-任务内自增。
- 显示可格式化为 `P001 / T01 / 阶段1`（内部仍存整数）。

## 5. 计时服务（Rust）

- **状态**：`Idle | Running`（仅两态，无暂停）。
- **总计时器**：`start` 时写 `started_at`（Rust 持有系统时间，不依赖前端定时器）；`stop` 时写 `ended_at`、`duration`，新增一条 stage 并递增 `stage_id`，同时更新 `project_task.updated_at`。
- 前端的 00:00 仅为显示：Rust 每秒 `emit("tick", {elapsed})`，刷新时分与圆点闪烁。
- **提醒计时器（看门狗）**：
  - 配置项：`异常时段 range`（如工作日 19:00 之后，或自定义时段）、`阈值 x`（异常时段内连续工作上限，分钟）、`倒计时 y`（自动停止前等待秒数）。
  - 进入异常时段且累计运行 > x：发 `show_remind` → 前端弹 P9「是否忘记停止，y 秒后自动停止」。
  - 用户手动关闭：重置提醒计时（重新累计 x）；无操作：y 后自动 `stop` → 进入 F3。
- 计时在后台 / 窗口隐藏时仍准确（基于系统时间戳，而非累加 tick）。

## 6. 数据管理（F4）

- 默认：`status='未完成'`，`ORDER BY updated_at DESC`（最新在最上）。
- 列：项目名 / 任务名 / DDL / 优先级 / 状态。
- 筛选（项目名、优先级、状态）与 DDL 三态排序在 Rust 侧拼参数化 SQL（防注入）。
- 单击选中；双击 / 对已选中行再单击 → 滚动定位 + P5-2 详情。
- 详情编辑：`保存` 事务更新；`取消` 丢弃；有未保存改动时前端标记 dirty，其他操作需确认。
- 导出：
  - **Excel**：Rust 用 `rust_xlsxwriter` 生成工作簿；**CSV**：Rust 直接拼装；均通过 `tauri-plugin-dialog` 弹系统保存对话框写盘。
  - **图表图片**：ECharts 在 WebView 内 `getDataURL` 得到 base64，交 Rust 解码后保存为 PNG。
  - **查看数据库**：提供「打开数据文件所在目录」命令（系统文件管理器）。

## 7. 统计看板（F5）

- 周期：当日 / 本周 / 本月 / 上周 / 上月；**周、月按自然周（周一为起点）、自然月（1 日）**。
- Rust 侧按时间窗聚合，返回结构化数据：
  - 甘特：按项目（纵轴）、任务条，日周期横轴为时、周/月为日。
  - 扇形：按项目聚合总时长占比；项目内任务占比由 WebView 二次拆分（悬浮显示）。
- 图表导出：ECharts `getDataURL` 取出图片，Rust 经保存对话框落盘；聚合数据可导出 CSV。

## 8. 报告生成与 LLM

报告生成走**两段式**：`固定格式化模板配置` 渲染出初稿 →（可选）`报告撰写 Skill` 做语言层改写。两个配置文件随应用分发、放在用户配置目录，可直接编辑。

**8.1 两个配置文件**

| 文件 | 角色 | 内容 |
| --- | --- | --- |
| `report-template.yaml` | 固定格式化模板配置 | 章节顺序、取数字段、字段排版、空值占位、时长 / 占比 / 日期格式、硬性取数规则、AI 参数 |
| `report-skill.md` | 报告撰写 Skill | 改写步骤、硬性约束（不得增删事实 / 改数字）、文风、篇幅、输出格式 |

- **职责边界**：模板只管结构与格式，Skill 只管表达；Skill 不得增删模板章节。
- **文件位置**：随包资源在 `resources/`；首次启动复制到应用配置目录
  - macOS：`~/Library/Application Support/workLog/config/`
  - Windows：`%APPDATA%\workLog\config\`
  - 用户改坏了可在设置里「恢复默认」（重新复制）。
- **读取时机**：生成报告时读取并缓存（按文件 mtime 失效）；解析用 `serde_yaml`，任一文件缺失 / 解析失败 → 回退内置默认模板，报告照常生成。

**8.2 生成流程**

1. Rust 按周期聚合阶段明细与项目-任务数据；
2. Rust 读 `report-template.yaml` → 渲染初稿（`{project_name}`、`{duration}`、`{result}` 等占位符按 `formats` 规则替换）；
3. 点「AI 增强」→ 若已配置 API Key：组装 prompt = **Skill 正文** + **模板初稿** + **结构化数据（JSON）** + **周期信息**，走 OpenAI 兼容 `POST /v1/chat/completions`；
4. 失败 / 未配置 Key / 模型输出异常 → **回退初稿**，不阻塞；
5. 前端展示并支持一键复制。

**8.3 其他**

- 复制：WebView 剪贴板（应用内，无浏览器）；报告可导出文本。
- 安全：Key 首版存本地配置文件，后续迁移系统钥匙串（macOS Keychain / Windows Credential Manager）。

**8.4 随包提供的默认文件内容**

`resources/report-template.yaml`：

```yaml
meta:
  version: 1
  name: 默认报告模板
  locale: zh-CN
  timezone: local
  encoding: UTF-8

periods:
  当日:
    title: "{date} 工作日报"
    label: 当日
    sections: [investment, main_output, next_goal, todo]
  本周:
    title: "第 {week} 周 周报"
    label: 本周
    sections: [investment, main_output, next_goal, todo]
  本月:
    title: "{year} 年 {month} 月 月报"
    label: 本月
    sections: [investment, main_output, next_goal, todo]
  上周:
    title: "第 {week} 周 周报"
    label: 上周
    sections: [investment, main_output, next_goal, todo]
  上月:
    title: "{year} 年 {month} 月 月报"
    label: 上月
    sections: [investment, main_output, next_goal, todo]

sections:
  - id: investment
    title: "{period_label}投入"
    type: paragraph
    source: stages
    group_by: project
    item_template: "{project_name} · {task_name}（{duration}）"
    joiner: "；"
    tail: "。"
    empty_text: "（本周期无记录）"
  - id: main_output
    title: 主要产出
    type: list
    source: stages
    item_template: "{result}"
    joiner: "；"
    dedupe: true
    max_items: 12
    empty_text: "（本周期暂无阶段成果）"
  - id: next_goal
    title: 下一目标
    type: list
    source: stages
    item_template: "{project_name} · {task_name}：{next_goal}"
    joiner: "；"
    dedupe: true
    max_items: 8
    empty_text: "（未填写下一目标）"
  - id: todo
    title: 待办与风险
    type: list
    source: project_task
    filter:
      status: 未完成
    sort:
      by: priority
      order: asc
    item_template: "[{priority}] {project_name} · {task_name}（DDL {due_date}）"
    empty_text: "（无未完成事项）"

formats:
  date: "YYYY-MM-DD"
  duration: "{hours} 小时 {minutes} 分钟"
  percent: "{value}%"
  int: "{value}"

rules:
  - 只使用数据源里真实存在的字段，不推断、不补写未记录的内容
  - 阶段成果 / 下一目标为空时使用该章节的 empty_text 占位，不留空行
  - 时长统一走 formats.duration，不出现「3.5h」「3小时30分」等混合格式
  - 占比四舍五入保留一位小数，各项占比合计为 100%
  - 项目名 / 任务名使用数据库中的原始名称，不改写、不缩写
  - 章节顺序、标题文字以本文件的定义为准，AI 不得增删章节

ai:
  enabled: true
  skill_file: report-skill.md      # 与本文件同目录
  max_tokens: 2000
  temperature: 0.3
  keep_structure: true             # 保留本模板的章节与顺序
  fallback_to_template: true       # 调用失败时回退初稿
```

`resources/report-skill.md`：

```markdown
---
name: worklog-report-writer
description: 把 workLog 按 report-template.yaml 渲染出的报告初稿改写成简洁、专业的书面报告；只做表达层改写，不增删事实、不改动任何数字。
version: 1
applies_to: report generation
input: [template_draft, structured_data, period]
output: markdown_text
---

# 报告撰写 Skill

## 何时使用
在「AI 增强」被触发、且应用已配置 API Key 时使用。调用方会把三样东西交给你：
1. **模板初稿**（template_draft）：已经按 report-template.yaml 渲染好的结构化文本；
2. **结构化数据**（structured_data）：该周期的阶段明细、项目-任务、聚合统计；
3. **周期信息**（period）：周期类型、日期范围、时区。

## 目标
在完全保留事实与结构的前提下，把初稿改写成可以直接发出去的书面报告：语句通顺、条理清楚、长度合适。

## 处理步骤
1. 通读初稿与结构化数据，确认所有数字、时长、占比、日期、项目名、任务名；
2. 照抄初稿的章节顺序与标题文字，不增删章节、不改标题、不改顺序；
3. 逐章节改写表达：合并重复表述、去掉口语与冗余主语、调整语序；
4. 数字类内容（时长、占比、阶段数、日期、DDL）原样搬运，只允许改变呈现形式；
5. 初稿中的空值占位符原样保留，不要替用户编内容；
6. 输出纯文本 / Markdown，不加前言与结语。

## 硬性约束
- 不得新增输入中不存在的事实、推测、评价、风险判断或改进建议；
- 不得改写、换算、四舍五入任何数字与时长；
- 不得把「下一目标」写成承诺或对进度的判断；
- 不得合并 / 拆分项目名、任务名、阶段成果条目；
- 语言：简体中文书面语，陈述句；不使用第一人称；不使用感叹号、表情、网络用语；
- 篇幅：单个条目不超过 3 行；整体长度与初稿相当（可压缩 20% 以内，但不得删信息）。

## 风格示例
✅ 正确：今日投入：交通仿真平台二期 · 轨迹数据处理（3 小时 30 分钟）、车辆检测模型训练（2 小时）。
❌ 错误：今天效率爆表！🎉 顺利完成轨迹处理框架搭建，势头非常好！（新增了输入里没有的评价与情绪）

## 输出格式
与模板初稿完全一致的章节结构：<报告标题> + 各章节标题与正文。
```

**8.5 设置项对应**

- 「设置 · 报告生成配置」显示两个文件的绝对路径，并提供「打开」（`tauri-plugin-opener` / `open` crate 调系统默认程序）与「恢复默认」（重新复制随包文件）两个动作。

## 9. 窗口与托盘

- 两个窗口：**悬浮球（透明、无边框、置顶）** 与 **主面板**（可由悬浮球展开/收起）。
- 悬浮球支持拖动（更新窗口位置）；主面板上边缘与悬浮球中部重叠。
- 托盘：开始 / 退出 / 显示；关闭主窗口不退出进程（常驻）。
- Windows 依赖 WebView2（安装包检测引导）。

## 10. 打包发布

- `tauri build`：Windows 出 `.msi/.exe`，macOS 出 `.app/.dmg`（含签名，未签名需用户允许）。
- 数据库初始化在首次启动时执行（建表 + 迁移版本号）。
- 版本管理：迁移脚本带 `PRAGMA user_version`，升级走 migration。

## 11. 里程碑（建议）

| 阶段 | 内容                                              |
| ---- | ------------------------------------------------- |
| M1   | Tauri 骨架 + 两张表 + 项目-任务新建/编辑（F1/F4） |
| M2   | 计时服务 + 提醒看门狗 + 阶段记录（F2/F3）          |
| M3   | 数据看板 + 导出（F5 图表部分）                    |
| M4   | 固定模板 + LLM 增强报告 + 托盘/打包（F5 报告，含 report-template.yaml / report-skill.md 两个配置文件） |

## 12. 风险与对策

- 透明置顶窗口在全屏 / 多显示器的差异 → 开发期两平台实测。
- LLM 输出不稳定 → 模板约束 + 重试 + 可编辑。
- 忘记停止 → 看门狗 + 上限自动停止。
- WebView2 缺失 → 安装包检测引导。
- 自定义字段 → 首期以 `custom_fields` JSON 承载，避免频繁改表结构。
