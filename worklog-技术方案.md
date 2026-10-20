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
│  ├─ report-template.yaml     # 报告模板配置（字段映射 / 自定义字段 / 格式化文档）
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

报告生成走**两段式**：`报告模板配置`（字段映射 → 自定义字段 → 最终格式化文档）渲染出初稿 →（可选）`报告撰写 Skill` 做语言层改写。两个配置文件随应用分发、放在用户配置目录，可直接编辑。

**8.1 两个配置文件**

| 文件 | 角色 | 内容 |
| --- | --- | --- |
| `report-template.yaml` | 报告模板配置（三部分） | ① 数据库字段映射（替代字段 → 源字段）② 自定义字段（用替代字段拼字符串，可逐条展开成多行）③ 最终格式化文档（日报 / 周报 / 月报） |
| `report-skill.md` | 报告撰写 Skill | 改写步骤、硬性约束（不得增删事实 / 改数字）、文风、篇幅、输出格式 |

模板渲染的占位符约定：

- `{字段}`：单行标量替换；
- `{自定义字段}`：`text` 型替换成一段文字；`list` 型按记录逐条展开成多行；
- 正文多行用 YAML 块标量 `|` 书写，换行与缩进原样保留；同一行可连续引用多个字段；
- 取值为空：`list` 型跳过该条，`text` 型替换为空字符串。

- **职责边界**：模板只管结构与格式，Skill 只管表达；Skill 不得增删模板章节。
- **文件位置**：随包资源在 `resources/`；首次启动复制到应用配置目录
  - macOS：`~/Library/Application Support/workLog/config/`
  - Windows：`%APPDATA%\workLog\config\`
  - 用户改坏了可在设置里「恢复默认」（重新复制）。
- **读取时机**：生成报告时读取并缓存（按文件 mtime 失效）；解析用 `serde_yaml`，任一文件缺失 / 解析失败 → 回退内置默认模板，报告照常生成。

**8.2 生成流程**

1. Rust 按周期聚合阶段明细与项目-任务数据；
2. Rust 读 `report-template.yaml`：先用 `fields` 把数据库行转成「替代字段 → 值」的映射，再按 `custom` 逐条渲染，最后用 `documents[周期].正文` 的占位符拼出初稿；
3. 点「AI 增强」→ 若已配置 API Key：组装 prompt = **Skill 正文** + **模板初稿** + **结构化数据（JSON）** + **周期信息**，走 OpenAI 兼容 `POST /v1/chat/completions`；
4. 失败 / 未配置 Key / 模型输出异常 → **回退初稿**，不阻塞；
5. 前端展示并支持一键复制。

**8.3 其他**

- 复制：WebView 剪贴板（应用内，无浏览器）；报告可导出文本。
- 安全：Key 首版存本地配置文件，后续迁移系统钥匙串（macOS Keychain / Windows Credential Manager）。

**8.4 随包提供的默认文件内容**

`resources/report-template.yaml`：

```yaml
# ==============================================================
# workLog · 报告模板配置（report-template.yaml）
# --------------------------------------------------------------
# 报告由三部分拼出来：
#   1) 数据库字段映射：替代字段 → 数据库源字段（一般不用改）
#   2) 自定义字段：用替代字段拼出来的字符串（可逐条展开成多行）
#   3) 最终格式化文档：日报 / 周报 / 月报，用 {} 引用上面的字段
#
# 占位符约定：
#   {字段}        单行标量替换，例如 {项目名}
#   {自定义字段}   自定义字段：text 型替换成一段文字；
#                  list 型按记录逐条展开成多行（每条记录渲染一次 text）
#   正文多行用 YAML 块标量 | 书写，换行与缩进原样保留
#   取值为空时：list 型跳过该条，text 型替换为空字符串
#   同一行可以连续引用多个字段，如 "{项目名} · {任务名}：{阶段成果}"
#
# 报告只写两件事：工作成果、工作计划；时间 / 数字由系统按固定规则
# 给出，模板里不出现任何具体日期与时长字样。
# ==============================================================

meta:
  version: 2
  name: 默认报告模板
  locale: zh-CN

# --------------------------------------------------------------
# 1. 数据库字段映射
#    左侧 = 本文件内使用的「替代字段」，右侧 = 数据库源字段
#    增删字段时只改这一段，下面的自定义字段与文档不用动
# --------------------------------------------------------------
fields:
  日期: period_date
  周期: period_label
  项目名: project_name
  任务名: task_name
  阶段号: stage_id
  阶段成果: result
  下一目标: next_goal
  时长: duration
  优先级: priority
  截止日期: due_date
  任务状态: status
  总时长: agg_duration
  项目数: agg_project_count
  任务数: agg_task_count

# --------------------------------------------------------------
# 2. 自定义字段
#    text 型：整个周期只渲染一次
#    list 型：按记录逐条渲染，每条记录一行
#    skip_if_empty：指定的替代字段为空时跳过该条
#    distinct：去掉完全重复的行
# --------------------------------------------------------------
custom:
  工作成果:
    type: list
    source: stages
    text: "{项目名} · {任务名}：{阶段成果}"
    skip_if_empty: 阶段成果
    distinct: true
  工作计划:
    type: list
    source: stages
    text: "{项目名} · {任务名}：{下一目标}"
    skip_if_empty: 下一目标
    distinct: true
  投入概览:
    type: text
    source: aggregate
    text: "本期共投入 {总时长}，覆盖 {项目数} 个项目、{任务数} 个任务。"

# --------------------------------------------------------------
# 3. 最终格式化文档
#    日报 / 周报 / 月报各一份；正文用块标量 | 多行书写，
#    每个占位符独占一行时，展开后仍是多行结构
# --------------------------------------------------------------
documents:
  日报:
    标题: "{日期} 工作日报"
    正文: |
      【工作成果】
      {工作成果}

      【工作计划】
      {工作计划}
  周报:
    标题: "{周期} 工作周报"
    正文: |
      【本周工作成果】
      {工作成果}

      【下周工作计划】
      {工作计划}
  月报:
    标题: "{周期} 工作月报"
    正文: |
      【本月工作成果】
      {工作成果}

      【下月工作计划】
      {工作计划}
```

`resources/report-skill.md`：

```markdown
---
name: worklog-report-writer
description: 把 workLog 按 report-template.yaml 渲染出的报告初稿改写成简洁、专业的书面报告；只做表达层改写，不增删事实、不改动任何数字与条目。
version: 2
applies_to: report generation
input: [template_draft, structured_data, period]
output: markdown_text
---

# 报告撰写 Skill

## 何时使用

在「AI 增强」被触发、且应用已配置 API Key 时使用。调用方会交给你三样东西：

1. **模板初稿**（template_draft）：已按 `report-template.yaml` 渲染好的文本；
2. **结构化数据**（structured_data）：该周期的阶段明细、项目-任务、聚合统计；
3. **周期信息**（period）：周期类型与时间范围。

## 目标

报告围绕两件事展开：**工作成果**（做了什么、产出是什么）与**工作计划**（接下来要做什么）。在完全保留事实与结构的前提下，把初稿改写成可以直接发出去的书面报告：语句通顺、条理清楚、长度合适。

## 处理步骤

1. 通读初稿与结构化数据，确认每一条记录、每一个数字、每一个名称；
2. **照抄初稿的章节顺序与标题文字**，不增删章节、不改标题、不改顺序；
3. 逐条改写表达：合并重复表述、去掉口语与冗余主语、调整语序；
4. 数字、时长、日期、项目名、任务名、阶段成果**原样搬运**，只允许做等价呈现（如把时长写成「3 小时 30 分钟」这种统一写法）；
5. 初稿里的空值占位原样保留，不要替用户编内容；
6. **条目数量与顺序保持不变**，不得把两条合并成一条，也不得把一条拆成两条；
7. 输出纯文本 / Markdown，不加「以下是改写后的报告」这类前言，也不加结语、感想或额外建议。

## 硬性约束

- 不得新增输入中不存在的事实、推测、评价、风险判断或改进建议；
- 不得改写、换算、四舍五入任何数字、时长与日期；
- 不得把「工作计划」写成承诺、承诺性的时间点或对进度的判断；
- 不得合并 / 拆分项目名、任务名、阶段成果条目；
- 不得改动模板给出的章节标题与占位符结构；
- 语言：简体中文书面语，陈述句；不使用第一人称；不使用感叹号、表情、网络用语、夸张修辞；
- 篇幅：单条不超过 3 行；整体长度与初稿相当（可压缩 20% 以内，但不得删信息）。

## 风格示例

初稿（模板渲染结果，结构如下）：

```
【工作成果】
{项目名} · {任务名}：{阶段成果}
【工作计划】
{项目名} · {任务名}：{下一目标}
```

✅ 正确输出 —— 章节、条目、顺序都不变，只把口语换成书面语：

```
【工作成果】
完成{任务名}阶段，产出{阶段成果}（{项目名}）。
【工作计划】
{项目名} · {任务名}：{下一目标}
```

❌ 错误输出 —— 新增了输入里没有的评价与情绪，还把工作计划写成了承诺：

```
【工作成果】
今天效率爆表！🎉 成果非常棒！
【工作计划】
明天一定把剩下的全部搞定！
```

## 输出格式

与模板初稿完全一致的章节结构：报告标题 + 各章节标题与正文，逐条对应。
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
| M4   | 模板配置 + LLM 增强报告 + 托盘/打包（F5 报告，含 report-template.yaml / report-skill.md 两个配置文件） |

## 12. 风险与对策

- 透明置顶窗口在全屏 / 多显示器的差异 → 开发期两平台实测。
- LLM 输出不稳定 → 模板约束 + 重试 + 可编辑。
- 忘记停止 → 看门狗 + 上限自动停止。
- WebView2 缺失 → 安装包检测引导。
- 自定义字段 → 首期以 `custom_fields` JSON 承载，避免频繁改表结构。
