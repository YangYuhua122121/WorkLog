/* ==============================================================
   workLog · 数据访问层
   --------------------------------------------------------------
   · 桌面应用（Tauri）：走 invoke('命令名', 参数)
   · 直接用浏览器打开 src/index.html：走内存假数据，方便脱离应用看界面
   对外统一返回「原型同款」的数据形状：proj / task / ddl / pri / status …
   ============================================================== */

const T = window.__TAURI__;
export const isDesktop = !!(T && T.core && T.core.invoke);

const invoke = (cmd, args) => T.core.invoke(cmd, args);
const clone = v => JSON.parse(JSON.stringify(v));

/* ---------- 本地时间工具（假数据用） ---------- */
function pad(n) { return String(n).padStart(2, '0'); }
function dateOffset(offset) {
  const d = new Date();
  d.setDate(d.getDate() + offset);
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate());
}
function dtStr(offset, h, m) { return dateOffset(offset) + ' ' + pad(h) + ':' + pad(m) + ':00'; }
function nowStr() {
  const d = new Date();
  return dateOffset(0) + ' ' + pad(d.getHours()) + ':' + pad(d.getMinutes()) + ':' + pad(d.getSeconds());
}

/* ---------- 假数据（浏览器预览用） ---------- */
const MOCK_ROWS = [
  { projectId: 1, taskId: 1, proj: '交通仿真平台二期', task: '轨迹数据处理', ddl: dateOffset(41), pri: '高', status: '未完成', note: '核心链路', custom: {}, created: dtStr(-10, 9, 20), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(-2, 9, 0), endedAt: dtStr(-2, 11, 30), durSec: 9000, out: '完成数据接入与清洗', next: '验证数据集，训练 baseline' },
    { no: 2, startedAt: dtStr(0, 9, 0), endedAt: dtStr(0, 12, 30), durSec: 12600, out: '接入轨迹数据，跑通流程', next: '验证数据集，训练 baseline' },
    { no: 3, startedAt: dtStr(0, 14, 0), endedAt: dtStr(0, 16, 0), durSec: 7200, out: '完成轨迹处理框架搭建', next: '补充边界与异常用例' }
  ] },
  { projectId: 1, taskId: 2, proj: '交通仿真平台二期', task: '车辆检测模型训练', ddl: dateOffset(46), pri: '中', status: '未完成', note: '', custom: {}, created: dtStr(-9, 10, 0), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(0, 13, 0), endedAt: dtStr(0, 15, 0), durSec: 7200, out: '启动 baseline 模型训练', next: '完成第一轮模型评估' },
    { no: 2, startedAt: dtStr(-1, 9, 0), endedAt: dtStr(-1, 17, 0), durSec: 28800, out: 'baseline 模型完成首轮训练', next: '补齐训练数据' }
  ] },
  { projectId: 1, taskId: 3, proj: '交通仿真平台二期', task: '仿真参数标定', ddl: dateOffset(53), pri: '中', status: '未完成', note: '', custom: {}, created: dtStr(-8, 14, 30), lastOp: 0, stages: [] },
  { projectId: 2, taskId: 1, proj: '智能网联测试', task: '测试用例编写', ddl: dateOffset(31), pri: '中', status: '未完成', note: '', custom: {}, created: dtStr(-7, 9, 10), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(0, 16, 30), endedAt: dtStr(0, 18, 0), durSec: 5400, out: '完成核心链路测试用例', next: '补充 V2X 场景用例' },
    { no: 2, startedAt: dtStr(-3, 9, 0), endedAt: dtStr(-3, 15, 0), durSec: 21600, out: '完成高优先级用例覆盖', next: '补充边界用例' }
  ] },
  { projectId: 2, taskId: 2, proj: '智能网联测试', task: '数据标注', ddl: dateOffset(42), pri: '低', status: '已完成', note: '', custom: {}, created: dtStr(-11, 16, 0), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(-5, 10, 0), endedAt: dtStr(-5, 17, 0), durSec: 25200, out: '完成标注规范与首批标注', next: '抽检标注质量' }
  ] },
  { projectId: 2, taskId: 3, proj: '智能网联测试', task: 'V2X 场景测试', ddl: dateOffset(59), pri: '高', status: '未完成', note: '', custom: {}, created: dtStr(-6, 11, 20), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(-1, 13, 0), endedAt: dtStr(-1, 18, 0), durSec: 18000, out: '开始 V2X 场景验证', next: '完成场景用例' }
  ] },
  { projectId: 3, taskId: 1, proj: '月度报告整理', task: '周报汇总', ddl: dateOffset(36), pri: '低', status: '未完成', note: '', custom: {}, created: dtStr(-7, 17, 40), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(0, 18, 30), endedAt: dtStr(0, 19, 30), durSec: 3600, out: '整理本周产出', next: '' }
  ] },
  { projectId: 3, taskId: 2, proj: '月度报告整理', task: '月报初稿', ddl: dateOffset(39), pri: '中', status: '未完成', note: '', custom: {}, created: dtStr(-7, 17, 45), lastOp: 0, stages: [] },
  { projectId: 4, taskId: 1, proj: '数据中台建设', task: '接口对接', ddl: dateOffset(49), pri: '高', status: '未完成', note: '', custom: {}, created: dtStr(-12, 10, 0), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(-8, 9, 0), endedAt: dtStr(-8, 17, 0), durSec: 28800, out: '完成核心接口对接', next: '接口联调与数据校验' }
  ] },
  { projectId: 4, taskId: 2, proj: '数据中台建设', task: '数据校验', ddl: dateOffset(51), pri: '低', status: '未完成', note: '', custom: {}, created: dtStr(-12, 10, 10), lastOp: 0, stages: [] },
  { projectId: 5, taskId: 1, proj: '文档输出', task: '用户手册', ddl: dateOffset(56), pri: '低', status: '未完成', note: '', custom: {}, created: dtStr(-13, 9, 0), lastOp: 0, stages: [
    { no: 1, startedAt: dtStr(-9, 9, 0), endedAt: dtStr(-9, 17, 0), durSec: 28800, out: '完成用户手册初稿', next: '部署文档补充' }
  ] },
  { projectId: 5, taskId: 2, proj: '文档输出', task: '部署文档', ddl: dateOffset(57), pri: '低', status: '未完成', note: '', custom: {}, created: dtStr(-13, 9, 5), lastOp: 0, stages: [] }
].map((d, i) => ({
  ...d,
  id: d.projectId + '-' + d.taskId,
  lastOp: Date.now() - i * 3600 * 1000,
  totalSec: d.stages.reduce((a, s) => a + s.durSec, 0)
}));

const mock = {
  rows: clone(MOCK_ROWS),
  pending: null,
  running: null,
  tickTimer: null,
  config: {
    customFields: [],
    llm: { baseUrl: 'https://api.openai.com/v1', apiKey: '', model: 'gpt-4o-mini' },
    appearance: { theme: 'dark', fontSize: 16 },
    reminder: { workStart: '09:00', workEnd: '18:00', limitXMinutes: 60, stopYSeconds: 60 },
    report: { templateFile: 'config/report-template.yaml', skillFile: 'config/report-skill.md' },
    window: { x: 0, y: 0, saved: false }
  }
};

/* Rust 字段（snake_case → camelCase）→ 界面字段（原型同款） */
function parseCustom(v) {
  if (!v) return {};
  if (typeof v === 'object') return v;
  try { return JSON.parse(v); } catch (_) { return {}; }
}
function norm(r, stages) {
  return {
    id: r.id || (r.projectId + '-' + r.taskId),
    projectId: r.projectId, taskId: r.taskId,
    proj: r.projectName, task: r.taskName || '',
    ddl: r.dueDate || '', pri: r.priority, status: r.status,
    note: r.remark || '', custom: parseCustom(r.customFields),
    lastOp: Date.parse((r.updatedAt || '').replace(' ', 'T')) || 0,
    totalSec: r.totalSec || 0,
    stageCount: r.stageCount || 0,
    stages: stages || [],
  };
}
function normStage(s) {
  return {
    no: s.stageId,
    date: (s.endedAt || s.startedAt || '').slice(0, 10),
    durSec: s.duration || 0,
    out: s.result || '', next: s.nextGoal || '',
    status: s.status || '',
    startedAt: s.startedAt, endedAt: s.endedAt,
  };
}

function isoLocal(d) {
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()) + ' ' +
    pad(d.getHours()) + ':' + pad(d.getMinutes()) + ':' + pad(d.getSeconds());
}

/* ================= 浏览器预览：统计 / 报告 ================= */
function periodRange(key) {
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const monday = new Date(today); monday.setDate(today.getDate() - ((today.getDay() + 6) % 7));
  const first = new Date(today.getFullYear(), today.getMonth(), 1);
  const next = new Date(today.getFullYear(), today.getMonth() + 1, 1);
  const prev = new Date(today.getFullYear(), today.getMonth() - 1, 1);
  let start, end, label, doc;
  switch (key) {
    case '本周': start = monday; end = new Date(monday); end.setDate(end.getDate() + 7); label = fmtDate(monday) + ' ~ ' + fmtDate(new Date(monday.getTime() + 6 * 86400000)); doc = '周报'; break;
    case '本月': start = first; end = next; label = today.getFullYear() + '-' + pad(today.getMonth() + 1); doc = '月报'; break;
    case '上周': start = new Date(monday); start.setDate(start.getDate() - 7); end = monday; label = fmtDate(start) + ' ~ ' + fmtDate(new Date(monday.getTime() - 86400000)); doc = '周报'; break;
    case '上月': start = prev; end = first; label = prev.getFullYear() + '-' + pad(prev.getMonth() + 1); doc = '月报'; break;
    default: start = today; end = new Date(today); end.setDate(end.getDate() + 1); label = fmtDate(today); doc = '日报';
  }
  return { key: key || '当日', start, end, label, docKind: doc };
}
function fmtDate(d) { return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate()); }
function parseLocal(s) {
  const m = String(s || '').match(/^(\d{4})-(\d{2})-(\d{2})[ T](\d{2}):(\d{2})(?::(\d{2}))?/);
  if (!m) return null;
  return new Date(+m[1], +m[2] - 1, +m[3], +m[4], +m[5], +(m[6] || 0));
}
function fmtDur(sec) {
  if (sec <= 0) return '0 秒';
  const h = Math.floor(sec / 3600), m = Math.floor(sec % 3600 / 60), s = sec % 60;
  if (h > 0) return h + ' 小时' + (m ? ' ' + m + ' 分钟' : '');
  if (m > 0) return m + ' 分钟' + (s ? ' ' + s + ' 秒' : '');
  return s + ' 秒';
}
function mockRecords(period) {
  const out = [];
  for (const row of mock.rows) {
    for (const s of row.stages) {
      const end = parseLocal(s.endedAt);
      if (!end || end < period.start || end >= period.end) continue;
      const start = parseLocal(s.startedAt) || end;
      out.push({
        projectId: row.projectId, taskId: row.taskId,
        projectName: row.proj, taskName: row.task,
        stageId: s.no, startedAt: start, endedAt: end, duration: s.durSec,
        result: s.out || '', nextGoal: s.next || '',
        priority: row.pri, dueDate: row.ddl || ''
      });
    }
  }
  out.sort((a, b) => a.endedAt - b.endedAt);
  return out;
}
function axisValue(d, period, daily) {
  if (daily) {
    const day = new Date(period.start);
    return (d - day) / 3600000;
  }
  return (d - period.start) / 86400000;
}
function mockSummary(key) {
  const period = periodRange(key);
  const recs = mockRecords(period);
  const daily = period.docKind === '日报';
  const groups = new Map();
  for (const r of recs) {
    const k = r.projectId + '-' + r.taskId;
    const sRaw = axisValue(r.startedAt, period, daily);
    const eRaw = axisValue(r.endedAt, period, daily);
    const s = daily ? Math.max(0, sRaw) : Math.max(0, Math.floor(sRaw));
    const e = daily ? Math.min(24, eRaw) : Math.floor(eRaw) + 1;
    if (!groups.has(k)) groups.set(k, { projectId: r.projectId, projectName: r.projectName, taskName: r.taskName, start: s, end: e, duration: 0, order: r.startedAt });
    const g = groups.get(k);
    g.start = Math.min(g.start, s);
    g.end = Math.max(g.end, e);
    g.duration += r.duration;
  }
  let min = 0, span = 24;
  if (daily && recs.length) {
    min = Math.max(0, Math.floor(Math.min(...recs.map(r => axisValue(r.startedAt, period, true)))));
    const hi = Math.min(24, Math.max(...recs.map(r => axisValue(r.endedAt, period, true))));
    span = Math.max(0.5, Math.ceil(hi) - min);
  } else if (!daily) {
    span = Math.round((period.end - period.start) / 86400000);
  }
  const axis = { unit: daily ? '时' : '日', min, span, ticks: [], labels: [], grid: [] };
  if (daily) {
    const step = Math.max(1, Math.ceil(span / 4));
    axis.ticks.push(min);
    for (let v = Math.ceil(min / step) * step; v < min + span - 0.01; v += step) if (v > min + 0.01) axis.ticks.push(v);
    axis.ticks.push(min + span);
    axis.labels = axis.ticks.map(v => pad(Math.floor(v)) + ':' + pad(Math.round((v - Math.floor(v)) * 60) % 60));
    for (let h = Math.ceil(min); h <= Math.floor(min + span); h++) axis.grid.push(h);
  } else {
    const names = ['周一', '周二', '周三', '周四', '周五', '周六', '周日'];
    if (key === '本周' || key === '上周') {
      for (let i = 0; i < 7; i++) { axis.ticks.push(i + 0.5); axis.labels.push(names[i]); }
    } else {
      for (let d = 0; d < span; d += 5) { axis.ticks.push(d + 0.5); axis.labels.push((d + 1) + '日'); }
    }
    for (let d = 0; d <= span; d++) axis.grid.push(d);
  }
  // 周 / 月：项目 × 天 → 当天任务清单
  const dayMap = new Map();
  if (!daily) {
    for (const r of recs) {
      const sd = Math.max(0, Math.floor((r.startedAt - period.start) / 86400000));
      const ed = Math.floor((r.endedAt - period.start) / 86400000);
      const days = Math.max(1, ed - sd + 1);
      const share = Math.round(r.duration / days);
      for (let d = sd; d <= ed; d++) {
        const key = r.projectId + '-' + d;
        if (!dayMap.has(key)) dayMap.set(key, { projectId: r.projectId, offset: d, duration: 0, tasks: new Map() });
        const day = dayMap.get(key);
        day.duration += share;
        const tkey = r.taskId;
        if (!day.tasks.has(tkey)) day.tasks.set(tkey, { name: r.taskName || '（未命名任务）', duration: 0 });
        day.tasks.get(tkey).duration += share;
      }
    }
  }
  const projects = new Map();
  for (const g of groups.values()) {
    if (!projects.has(g.projectId)) projects.set(g.projectId, { name: g.projectName, order: g.order, tasks: [] });
    const p = projects.get(g.projectId);
    if (g.order < p.order) p.order = g.order;
    p.tasks.push(g);
  }
  const plist = [...projects.values()].sort((a, b) => a.order - b.order || a.name.localeCompare(b.name, 'zh'));
  const total = recs.reduce((a, r) => a + r.duration, 0);
  const rows = [], pie = [];
  plist.forEach((p, i) => {
    p.tasks.sort((a, b) => a.start - b.start);
    const colorIndex = i % 6 + 1;
    const dayTasks = daily ? [] : [...dayMap.values()]
      .filter(d => d.projectId === p.tasks[0].projectId)
      .sort((a, b) => a.offset - b.offset)
      .map(d => {
        const date = new Date(period.start.getTime() + d.offset * 86400000);
        return {
          start: d.offset, end: d.offset + 1,
          date: pad(date.getMonth() + 1) + '-' + pad(date.getDate()),
          duration: d.duration, durationText: fmtDur(d.duration),
          tasks: [...d.tasks.values()]
            .sort((a, b) => b.duration - a.duration)
            .map(t => ({ name: t.name, duration: t.duration, durationText: fmtDur(t.duration) }))
        };
      });
    rows.push({
      projectName: p.name, colorIndex,
      tasks: p.tasks.map(t => ({
        name: t.taskName || '（未命名任务）',
        start: t.start, end: Math.min(min + span, t.end - t.start < span * 0.012 ? t.start + span * 0.012 : t.end),
        duration: t.duration, durationText: fmtDur(t.duration)
      })),
      dayTasks
    });
    const projTotal = p.tasks.reduce((a, t) => a + t.duration, 0);
    pie.push({
      projectName: p.name, colorIndex, duration: projTotal,
      pct: total ? projTotal / total * 100 : 0, durationText: fmtDur(projTotal),
      tasks: p.tasks.map(t => ({
        name: t.taskName || '（未命名任务）', duration: t.duration,
        pct: total ? t.duration / total * 100 : 0, durationText: fmtDur(t.duration)
      }))
    });
  });
  return {
    period, axis, rows, pie,
    totalDuration: total, totalDurationText: fmtDur(total),
    projectCount: rows.length, taskCount: rows.reduce((a, r) => a + r.tasks.length, 0),
    recordCount: recs.length
  };
}
function mockReport(key) {
  const summary = mockSummary(key);
  const recs = mockRecords(periodRange(key));
  const ctx = {
    period_date: summary.period.label, period_label: summary.period.label,
    agg_total_duration: summary.totalDurationText,
    agg_project_count: String(summary.projectCount),
    agg_task_count: String(summary.taskCount)
  };
  const body = summary.period.docKind === '周报'
    ? '{周期} 工作周报\n\n【本周工作成果】\n{成果句}\n\n【下周工作计划】\n{计划句}'
    : summary.period.docKind === '月报'
      ? '{周期} 工作月报\n\n【本月工作成果】\n{成果句}\n\n【下月工作计划】\n{计划句}'
      : '{日期} 工作日报\n\n【工作成果】\n{成果句}\n\n【工作计划】\n{计划句}';
  const fields = { 日期: 'period_date', 周期: 'period_label', 项目名: 'project_name', 任务名: 'task_name', 成果: 'result', 下一步: 'next_goal' };
  const rv = (r, f) => ({ project_name: r.projectName, task_name: r.taskName, result: r.result, next_goal: r.nextGoal }[f] || '');
  const line = (pat) => recs.map(r => pat.replace(/\{([^}]+)\}/g, (m, k) => {
    const src = fields[k] || k;
    return src === 'period_date' || src === 'period_label' ? ctx[src] : rv(r, src);
  })).filter(s => !s.includes('：') || s.split('：').pop().trim()).filter((v, i, a) => a.indexOf(v) === i);
  let text = body.replace('{成果句}', line('{项目名} · {任务名}：{成果}').join('\n'))
    .replace('{计划句}', line('{项目名} · {任务名}：{下一步}').join('\n'))
    .replace('{日期}', ctx.period_date).replace('{周期}', ctx.period_label);
  return { text, usedFallback: false, templatePath: '（浏览器预览无文件）', skillPath: '（浏览器预览无文件）', period: summary.period };
}

/* ================= 对外接口 ================= */

export const api = {
  isDesktop,

  /* ---------- 数据表 ---------- */
  async listTasks() {
    if (isDesktop) {
      const rows = await invoke('list_tasks');
      return rows.map(r => norm(r, []));
    }
    return clone(mock.rows);
  },
  async getTask(projectId, taskId) {
    if (isDesktop) {
      const d = await invoke('get_task', { projectId, taskId });
      return { ...norm(d.row, []), stages: d.stages.map(normStage) };
    }
    const row = mock.rows.find(r => r.projectId === projectId && r.taskId === taskId);
    if (!row) throw new Error('任务不存在');
    return clone(row);
  },
  async createTask(input) {
    const payload = { ...input, customFields: JSON.stringify(input.customFields || {}) };
    if (isDesktop) {
      const r = await invoke('create_task', { input: payload });
      return norm(r, []);
    }
    const sameProj = mock.rows.filter(r => r.proj === input.projectName);
    const projectId = sameProj.length ? sameProj[0].projectId : Math.max(0, ...mock.rows.map(r => r.projectId)) + 1;
    const taskId = Math.max(0, ...sameProj.map(r => r.taskId)) + 1;
    const row = {
      id: projectId + '-' + taskId, projectId, taskId,
      proj: input.projectName, task: input.taskName || '',
      ddl: input.dueDate || '', pri: input.priority || '中', status: '未完成',
      note: input.remark || '', custom: input.customFields || {},
      lastOp: Date.now(), totalSec: 0, stages: [],
    };
    mock.rows.unshift(row);
    return clone(row);
  },
  async updateTask(projectId, taskId, input) {
    const payload = { ...input, customFields: JSON.stringify(input.customFields || {}) };
    if (isDesktop) { await invoke('update_task', { projectId, taskId, input: payload }); return; }
    const row = mock.rows.find(r => r.projectId === projectId && r.taskId === taskId);
    if (!row) throw new Error('任务不存在');
    row.proj = input.projectName; row.task = input.taskName || '';
    row.ddl = input.dueDate || ''; row.pri = input.priority || row.pri;
    row.status = input.status || row.status; row.note = input.remark || '';
    row.custom = input.customFields || row.custom;
    row.lastOp = Date.now();
  },
  async deleteTask(projectId, taskId) {
    if (isDesktop) { await invoke('delete_task', { projectId, taskId }); return; }
    mock.rows = mock.rows.filter(r => !(r.projectId === projectId && r.taskId === taskId));
  },
  async exportTasks(format) {
    if (isDesktop) return invoke('export_tasks', { format });
    const rows = mock.rows;
    const csv = '\ufeff项目号,任务号,项目名,任务名,状态,优先级,截止日期\n' +
      rows.map(r => [r.projectId, r.taskId, r.proj, r.task, r.status, r.pri, r.ddl].join(',')).join('\n');
    const blob = new Blob([csv], { type: 'text/csv;charset=utf-8' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'workLog-项目任务.csv';
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    return '浏览器下载';
  },
  async openDataDir() {
    if (isDesktop) { await invoke('open_data_dir'); return; }
    throw new Error('浏览器预览下没有本地数据库');
  },

  /* ---------- 阶段 ---------- */
  async addStage(input) {
    if (isDesktop) { await invoke('add_stage', { input }); return; }
    const row = mock.rows.find(r => r.projectId === input.projectId && r.taskId === input.taskId);
    if (!row) throw new Error('任务不存在');
    const no = row.stages.length + 1;
    row.stages.push({
      no, date: (input.endedAt || '').slice(0, 10), durSec: input.duration || 0,
      out: input.result || '', next: input.nextGoal || '',
      startedAt: input.startedAt, endedAt: input.endedAt, status: input.status || '未完成'
    });
    row.totalSec = row.stages.reduce((a, s) => a + s.durSec, 0);
    row.status = input.status || row.status;
    row.lastOp = Date.now();
  },
  async updateStage(projectId, taskId, stageId, patch) {
    if (isDesktop) { await invoke('update_stage', { projectId, taskId, stageId, patch }); return; }
    const row = mock.rows.find(r => r.projectId === projectId && r.taskId === taskId);
    const s = row && row.stages.find(x => x.no === stageId);
    if (!s) throw new Error('阶段不存在');
    if (patch.result != null) s.out = patch.result;
    if (patch.nextGoal != null) s.next = patch.nextGoal;
    if (patch.status != null) s.status = patch.status;
  },

  /* ---------- 计时 ---------- */
  async startTimer(projectId, taskId) {
    if (isDesktop) { await invoke('start_timer', { projectId, taskId }); return; }
    if (mock.pending) throw new Error('请先保存或删除上次阶段记录');
    if (mock.running) throw new Error('已经在计时了');
    const started = Date.now();
    mock.running = { projectId, taskId, started };
    clearInterval(mock.tickTimer);
    mock.tickTimer = setInterval(() => {
      const cb = api._tickCb;
      if (cb) cb(Math.floor((Date.now() - started) / 1000));
    }, 1000);
  },
  async stopTimer() {
    if (isDesktop) return invoke('stop_timer');
    if (!mock.running) throw new Error('当前没有在计时');
    const now = new Date();
    const started = new Date(mock.running.started);
    const pending = {
      projectId: mock.running.projectId, taskId: mock.running.taskId,
      startedAt: isoLocal(started), endedAt: isoLocal(now),
      duration: Math.floor((now - started) / 1000),
    };
    mock.pending = pending;
    mock.running = null;
    clearInterval(mock.tickTimer); mock.tickTimer = null;
    return pending;
  },
  async timerState() {
    if (isDesktop) return invoke('timer_state');
    return {
      running: !!mock.running, elapsed: mock.running ? Math.floor((Date.now() - mock.running.started) / 1000) : 0,
      projectId: mock.running ? mock.running.projectId : null,
      taskId: mock.running ? mock.running.taskId : null,
      hasPending: !!mock.pending, pending: mock.pending, remindActive: false, remindRemaining: 0
    };
  },
  async discardPending() {
    if (isDesktop) { await invoke('discard_pending'); return; }
    mock.pending = null;
  },
  async keepWorking() {
    if (isDesktop) { await invoke('keep_working'); }
  },
  _tickCb: null,
  onTick(cb) {
    api._tickCb = cb;
    if (isDesktop) T.event.listen('timer-tick', e => cb(e.payload));
  },
  onReminderShow(cb) { if (isDesktop) T.event.listen('reminder-show', e => cb(e.payload)); },
  onReminderTick(cb) { if (isDesktop) T.event.listen('reminder-tick', e => cb(e.payload)); },
  onReminderHide(cb) { if (isDesktop) T.event.listen('reminder-hide', () => cb()); },
  onAutoStopped(cb) { if (isDesktop) T.event.listen('timer-auto-stopped', e => cb(e.payload)); },
  onTrayToggle(cb) { if (isDesktop) T.event.listen('tray-toggle-timer', () => cb()); },

  /* ---------- 双窗口事件 ---------- */
  emit(name, payload) {
    if (!isDesktop) return;
    try { T.event.emit(name, payload); } catch (_) {}
  },
  onEvent(name, cb) {
    if (!isDesktop) return;
    try { T.event.listen(name, e => cb(e.payload)); } catch (_) {}
  },

  /* ---------- 统计 / 报告 ---------- */
  async getSummary(period) {
    if (isDesktop) return invoke('get_summary', { period });
    return mockSummary(period);
  },
  async generateReport(period) {
    if (isDesktop) return invoke('generate_report', { period });
    return mockReport(period);
  },
  async aiRewrite(period, draft) {
    if (isDesktop) return invoke('ai_rewrite_report', { period, draft });
    throw new Error('浏览器预览不支持 AI 增强，请在桌面应用中配置 API Key');
  },

  /* ---------- 配置 / 文件 ---------- */
  async getConfig() {
    if (isDesktop) return invoke('get_config');
    return clone(mock.config);
  },
  async saveConfig(cfg) {
    if (isDesktop) { await invoke('save_config', { cfg }); return; }
    mock.config = clone(cfg);
  },
  async appInfo() {
    if (isDesktop) return invoke('app_info');
    return { version: 'browser', dbPath: '', configDir: '', templatePath: '（浏览器预览无文件）', skillPath: '（浏览器预览无文件）' };
  },
  async openConfigFile(relPath) {
    if (isDesktop) { await invoke('open_config_file', { relPath }); return; }
    throw new Error('浏览器预览下无法打开文件');
  },
  async resetReportConfig(kind) {
    if (isDesktop) { await invoke('reset_report_config', { kind }); return; }
    throw new Error('浏览器预览下无法恢复默认文件');
  },

  /* ---------- 窗口 ---------- */
  async setWindowSize(width, height) {
    if (!isDesktop) return;
    try {
      const win = T.window.getCurrentWindow();
      await win.setSize(new T.window.LogicalSize(width, height));
    } catch (_) { /* 忽略 */ }
  },
  async setWindowPosition(x, y) {
    if (!isDesktop) return;
    try {
      const win = T.window.getCurrentWindow();
      await win.setPosition(new T.window.PhysicalPosition(Math.round(x), Math.round(y)));
    } catch (_) { /* 忽略 */ }
  },
  async windowOuterPosition() {
    if (!isDesktop) return null;
    try { return await T.window.getCurrentWindow().outerPosition(); } catch (_) { return null; }
  },
  async monitor() {
    if (!isDesktop) return null;
    try { return await (T.window.currentMonitor ? T.window.currentMonitor() : T.window.primaryMonitor()); } catch (_) { return null; }
  },
  async showBoardWindow() {
    if (!isDesktop) return;
    try { await invoke('show_board'); } catch (_) { /* 忽略 */ }
  },
  async hideBoardWindow() {
    if (!isDesktop) return;
    try { await invoke('hide_board'); } catch (_) { /* 忽略 */ }
  },
  async resizeClipWindow(width, height) {
    if (!isDesktop) return;
    try { await invoke('resize_clip_window', { width, height }); } catch (_) { /* 忽略 */ }
  },
  async startWindowDrag() {
    if (!isDesktop) return;
    try { await T.window.getCurrentWindow().startDragging(); } catch (_) { /* 忽略 */ }
  },
  async saveWindowPosition(x, y) {
    if (!isDesktop) return;
    try { await invoke('save_window_pos', { x: Math.round(x), y: Math.round(y) }); } catch (_) { /* 忽略 */ }
  },
  onWindowMoved(cb) {
    if (!isDesktop) return;
    try { T.window.getCurrentWindow().onMoved(e => cb(e.payload)); } catch (_) { /* 忽略 */ }
  },
};
