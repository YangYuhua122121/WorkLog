/* ==============================================================
   workLog · 数据访问层
   --------------------------------------------------------------
   · 桌面应用（Tauri）：走 invoke('命令名', 参数)
   · 直接用浏览器打开 src/index.html：走内存假数据，方便脱离应用看界面
   对外统一返回「原型同款」的数据形状：proj / task / ddl / pri / status …
   ============================================================== */

const T = window.__TAURI__;
export const isDesktop = !!(T && T.core);

const invoke = (cmd, args) => T.core.invoke(cmd, args);

/* ---------- 假数据（浏览器预览用） ---------- */
const MOCK_ROWS = [
  {id:1, projectId:1, taskId:1, proj:'交通仿真平台二期', task:'轨迹数据处理', ddl:'2026-11-30', pri:'高', status:'未完成', note:'核心链路', custom:{},
   created:'2026-10-10 09:20', lastOp:0, totalSec:0, stages:[
     {no:1, date:'2026-10-16', durSec:14400, out:'完成数据接入与清洗', next:'验证数据集，训练 baseline'},
     {no:2, date:'2026-10-17', durSec:10800, out:'搭建轨迹处理框架', next:'接入轨迹数据并验证'},
     {no:3, date:'2026-10-18', durSec:12600, out:'接入轨迹数据，跑通流程', next:'验证数据集，训练 baseline'}
   ]},
  {id:2, projectId:1, taskId:2, proj:'交通仿真平台二期', task:'车辆检测模型训练', ddl:'2026-12-05', pri:'中', status:'未完成', note:'', custom:{},
   created:'2026-10-11 10:00', lastOp:0, totalSec:0, stages:[
     {no:1, date:'2026-10-18', durSec:18000, out:'训练 baseline 模型', next:'评估指标、对比实验'}
   ]},
  {id:3, projectId:1, taskId:3, proj:'交通仿真平台二期', task:'仿真参数标定', ddl:'2026-12-12', pri:'中', status:'未完成', note:'', custom:{},
   created:'2026-10-12 14:30', lastOp:0, totalSec:0, stages:[]},
  {id:4, projectId:2, taskId:1, proj:'智能网联测试', task:'测试用例编写', ddl:'2026-11-20', pri:'中', status:'未完成', note:'', custom:{},
   created:'2026-10-13 09:10', lastOp:0, totalSec:0, stages:[
     {no:1, date:'2026-10-18', durSec:21600, out:'编写高优先级用例', next:'补充边界与异常用例'}
   ]},
  {id:5, projectId:2, taskId:2, proj:'智能网联测试', task:'数据标注', ddl:'2026-12-01', pri:'低', status:'已完成', note:'', custom:{},
   created:'2026-10-09 16:00', lastOp:0, totalSec:0, stages:[]},
  {id:6, projectId:2, taskId:3, proj:'智能网联测试', task:'V2X 场景测试', ddl:'2026-12-18', pri:'高', status:'未完成', note:'', custom:{},
   created:'2026-10-14 11:20', lastOp:0, totalSec:0, stages:[]},
  {id:7, projectId:3, taskId:1, proj:'月度报告整理', task:'周报汇总', ddl:'2026-11-25', pri:'低', status:'未完成', note:'', custom:{},
   created:'2026-10-13 17:40', lastOp:0, totalSec:0, stages:[]},
  {id:8, projectId:3, taskId:2, proj:'月度报告整理', task:'月报初稿', ddl:'2026-11-28', pri:'中', status:'未完成', note:'', custom:{},
   created:'2026-10-13 17:45', lastOp:0, totalSec:0, stages:[]},
  {id:9, projectId:4, taskId:1, proj:'数据中台建设', task:'接口对接', ddl:'2026-12-08', pri:'高', status:'未完成', note:'', custom:{},
   created:'2026-10-08 10:00', lastOp:0, totalSec:0, stages:[]},
  {id:10,projectId:4, taskId:2, proj:'数据中台建设', task:'数据校验', ddl:'2026-12-10', pri:'低', status:'未完成', note:'', custom:{},
   created:'2026-10-08 10:10', lastOp:0, totalSec:0, stages:[]},
  {id:11,projectId:5, taskId:1, proj:'文档输出', task:'用户手册', ddl:'2026-12-15', pri:'低', status:'未完成', note:'', custom:{},
   created:'2026-10-07 09:00', lastOp:0, totalSec:0, stages:[]},
  {id:12,projectId:5, taskId:2, proj:'文档输出', task:'部署文档', ddl:'2026-12-16', pri:'低', status:'未完成', note:'', custom:{},
   created:'2026-10-07 09:05', lastOp:0, totalSec:0, stages:[]}
]
.map((d,i)=>({...d, lastOp: Date.now()-i*3600*1000, totalSec: d.stages.reduce((a,s)=>a+s.durSec,0)}));

const mock = {
  rows: JSON.parse(JSON.stringify(MOCK_ROWS)),
  nextId: 100,
  running: null,
  pending: null,
  tickTimer: null,
  config: {
    llm: { baseUrl: 'https://api.openai.com/v1', apiKey: '', model: 'gpt-4o-mini' },
    appearance: { theme: 'dark', fontSize: 16 },
    reminder: { workStart: '09:00', workEnd: '18:00', limitXMinutes: 60, stopYSeconds: 60 },
    report: { templateFile: 'config/report-template.yaml', skillFile: 'config/report-skill.md' },
  },
};

const clone = v => JSON.parse(JSON.stringify(v));

/* Rust 字段（snake_case）→ 界面字段（原型同款） */
function norm(r, stages) {
  return {
    id: r.id,
    projectId: r.projectId, taskId: r.taskId,
    proj: r.projectName, task: r.taskName || '',
    ddl: r.dueDate || '', pri: r.priority, status: r.status,
    note: r.remark || '', custom: r.customFields || {},
    lastOp: Date.parse(r.updatedAt || '') || 0,
    totalSec: r.totalSec || 0,
    stages: stages || [],
  };
}
function normStage(s) {
  return {
    no: s.stageId,
    date: (s.endedAt || s.startedAt || '').slice(0, 10),
    durSec: s.duration || 0,
    out: s.result || '', next: s.nextGoal || '',
    startedAt: s.startedAt, endedAt: s.endedAt,
  };
}

/* ================= 对外接口 ================= */

export const api = {
  isDesktop,

  /** 数据表全部行 */
  async listTasks() {
    if (isDesktop) {
      const rows = await invoke('list_tasks');
      return rows.map(r => norm(r, []));
    }
    return clone(mock.rows);
  },

  /** 详情（含阶段列表） */
  async getTask(projectId, taskId) {
    if (isDesktop) {
      const d = await invoke('get_task', { projectId, taskId });
      return { ...norm(d.row, []), stages: d.stages.map(normStage) };
    }
    const row = mock.rows.find(r => r.projectId === projectId && r.taskId === taskId);
    if (!row) throw new Error('任务不存在');
    return clone(row);
  },

  /** 新建项目 - 任务 */
  async createTask(input) {
    if (isDesktop) {
      const r = await invoke('create_task', { input });
      return norm(r, []);
    }
    const sameProj = mock.rows.filter(r => r.proj === input.projectName);
    const projNo = sameProj.length ? sameProj[0].projectId : Math.max(0, ...mock.rows.map(r => r.projectId)) + 1;
    const taskNo = Math.max(0, ...sameProj.map(r => r.taskId)) + 1;
    const row = {
      id: String(mock.nextId++), projectId: projNo, taskId: taskNo,
      proj: input.projectName, task: input.taskName || '',
      ddl: input.dueDate || '', pri: input.priority || '中', status: '未完成',
      note: input.remark || '', custom: input.customFields || {},
      lastOp: Date.now(), totalSec: 0, stages: [],
    };
    mock.rows.unshift(row);
    return clone(row);
  },

  /** 保存详情 */
  async updateTask(projectId, taskId, input) {
    if (isDesktop) {
      await invoke('update_task', { projectId, taskId, input });
      return;
    }
    const row = mock.rows.find(r => r.projectId === projectId && r.taskId === taskId);
    if (!row) throw new Error('任务不存在');
    row.proj = input.projectName; row.task = input.taskName || '';
    row.ddl = input.dueDate || ''; row.pri = input.priority || row.pri;
    row.status = input.status || row.status; row.note = input.remark || '';
    row.custom = input.customFields || row.custom;
    row.lastOp = Date.now();
  },

  /** 新增一条阶段（F3 保存） */
  async addStage(input) {
    if (isDesktop) {
      await invoke('add_stage', { input });
      return;
    }
    const row = mock.rows.find(r => r.projectId === input.projectId && r.taskId === input.taskId);
    if (!row) throw new Error('任务不存在');
    const no = row.stages.length + 1;
    row.stages.push({
      no, date: (input.endedAt || '').slice(0, 10) || new Date().toISOString().slice(0, 10),
      durSec: input.duration || 0, out: input.result || '', next: input.nextGoal || '',
      startedAt: input.startedAt, endedAt: input.endedAt,
    });
    row.totalSec = row.stages.reduce((a, s) => a + s.durSec, 0);
    row.lastOp = Date.now();
  },

  /* ---------- 计时 ---------- */
  async startTimer(projectId, taskId) {
    if (isDesktop) { await invoke('start_timer', { projectId, taskId }); return; }
    if (mock.running) throw new Error('已经在计时了');
    const started = Date.now();
    mock.running = { projectId, taskId, started };
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
      startedAt: started.toISOString().slice(0, 19).replace('T', ' '),
      endedAt: now.toISOString().slice(0, 19).replace('T', ' '),
      duration: Math.floor((now - started) / 1000),
    };
    mock.pending = pending;
    mock.running = null;
    clearInterval(mock.tickTimer); mock.tickTimer = null;
    return pending;
  },
  /** 计时状态（窗口重开 / 刷新后恢复显示） */
  async timerState() {
    if (isDesktop) return invoke('timer_state');
    return { running: !!mock.running, elapsed: 0, projectId: null, taskId: null, hasPending: !!mock.pending };
  },
  async discardPending() {
    if (isDesktop) { await invoke('discard_pending'); return; }
    mock.pending = null;
  },
  _tickCb: null,
  /** 计时心跳 */
  onTick(cb) {
    api._tickCb = cb;
    if (isDesktop) T.event.listen('timer-tick', e => cb(e.payload));
  },

  /* ---------- 配置 ---------- */
  async getConfig() {
    if (isDesktop) return invoke('get_config');
    return clone(mock.config);
  },
  async saveConfig(cfg) {
    if (isDesktop) { await invoke('save_config', { cfg }); return; }
    mock.config = clone(cfg);
  },
  async openConfigFile(relPath) {
    if (isDesktop) { await invoke('open_config_file', { relPath }); return; }
    throw new Error('浏览器预览下无法打开文件');
  },

  /* ---------- 窗口（展开 / 收起时改窗口大小） ---------- */
  async setWindowSize(width, height) {
    if (!isDesktop) return;
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setSize({ width, height });
    } catch (_) { /* 忽略 */ }
  },
};
