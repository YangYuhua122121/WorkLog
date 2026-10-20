/* ==============================================================
   workLog · 前端逻辑
   --------------------------------------------------------------
   数据一律经 api 层拿：桌面应用走 SQLite（Rust 侧），
   直接用浏览器打开时走内存假数据，方便脱离应用看界面。
   ============================================================== */
import { api } from './api.js';

/** 当前数据表内容（由 reloadRows 填充） */
let DATA = [];

const $  = s => document.querySelector(s);
const $$ = s => Array.from(document.querySelectorAll(s));

const state = {
  themePref: 'dark',
  proj: '全部', pri: '全部', status: '未完成',
  sortKey: '', sortDir: 0,
  selectedId: null,
  timerRunning: false, seconds: 0, timerTaskId: null,
  clampEnabled: true, savedScrollTop: 0,
  showNewRow: false, modalLocked: false,
  detailDirty: false, editingId: null, editingStage: null, loadingForm: false,
  detailSnapshot: null, customFields: [],
  pendingStage: null,
  config: null, appInfo: null,
  period: '当日', lastSummary: null, reportText: '', aiBusy: false,
  boardOpen: false, ready: false,
};

/* =====================================================================
   通用工具
   ===================================================================== */
function esc(v) {
  return String(v == null ? '' : v)
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;').replace(/'/g, '&#39;');
}
let toastTimer = null;
function toast(msg) {
  const t = $('#toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove('show'), 2000);
}
function projectNames() {
  return [...new Set(DATA.map(d => d.proj))].sort((a, b) => a.localeCompare(b, 'zh'));
}
function rowById(id) {
  return DATA.find(d => d.id === String(id));
}
function ddlDays(ddl) {
  if (!ddl) return null;
  const d = new Date(ddl + 'T00:00:00');
  const today = new Date(); today.setHours(0, 0, 0, 0);
  return Math.round((d - today) / 86400000);
}
function ddlText(ddl) {
  const n = ddlDays(ddl);
  if (n === null) return '—';
  if (n < 0) return '逾期 ' + Math.abs(n) + ' 天';
  if (n === 0) return '今天';
  return n + ' 天';
}
function fmtHM(sec) {
  const h = Math.floor(sec / 3600), m = Math.floor(sec % 3600 / 60);
  return String(h).padStart(2, '0') + ':' + String(m).padStart(2, '0');
}
function fmtDur(sec) {
  if (sec <= 0) return '0 秒';
  const h = Math.floor(sec / 3600), m = Math.floor(sec % 3600 / 60), s = sec % 60;
  if (h > 0) return h + ' 小时' + (m ? ' ' + m + ' 分钟' : '');
  if (m > 0) return m + ' 分钟' + (s ? ' ' + s + ' 秒' : '');
  return s + ' 秒';
}
function pad(n) { return String(n).padStart(2, '0'); }
function todayStr() {
  const d = new Date();
  return d.getFullYear() + '-' + pad(d.getMonth() + 1) + '-' + pad(d.getDate());
}
function activeTri(sel) {
  const b = $(sel + ' button.active');
  return b ? b.dataset.v : '';
}
function fillTri(sel, value) {
  $$(sel + ' button').forEach(b => b.classList.toggle('active', b.dataset.v === value));
}

/* =====================================================================
   主面板窗口（双窗口架构）
   --------------------------------------------------------------
   悬浮球在独立的 `clip` 窗口；主面板是固定的 480×680 窗口。
   展开 = 显示主面板窗口 + 板子从上向下滑出；收起 = 反向滑回后隐藏窗口。
   不再 resize 任何窗口，因此不会闪。
   ===================================================================== */
const unit = $('#unit');
let boardCloseTimer = null;

function nextPaint() {
  return new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
}

async function openBoardNow() {
  clearTimeout(boardCloseTimer);
  if (state.boardOpen) {
    unit.classList.add('is-open');
    return;
  }
  state.boardOpen = true;
  await api.showBoardWindow();
  await nextPaint();
  if (state.boardOpen) unit.classList.add('is-open');
}

function closeBoardNow() {
  if ($('#remind9')?.classList.contains('open')) return;
  if (state.modalLocked) {
    toast('请先保存本次阶段记录');
    api.emit('clip-toast', '请先保存本次阶段记录');
    return;
  }
  const hadSheet = anySheetOpen();
  closeAllSheets();
  state.showNewRow = false;
  state.detailDirty = false;
  state.editingId = null;
  state.detailSnapshot = null;
  state.clampEnabled = true;
  state.boardOpen = false;
  unit.classList.remove('is-open');
  if (hadSheet) restoreScroll();
  clearTimeout(boardCloseTimer);
  boardCloseTimer = setTimeout(async () => {
    if (state.boardOpen) return;
    renderTable();
    await api.hideBoardWindow();
  }, 310);
}

function toggleBoard() {
  if ($('#remind9')?.classList.contains('open')) {
    toast('请先在提醒窗选择「仍在工作」');
    return;
  }
  if (state.modalLocked) {
    toast('请先保存本次阶段记录');
    api.emit('clip-toast', '请先保存本次阶段记录');
    return;
  }
  if (state.detailDirty) {
    toast('有未保存的修改，请先保存或取消');
    api.emit('clip-toast', '有未保存的修改，请先保存或取消');
    return;
  }
  if (state.boardOpen) closeBoardNow();
  else openBoardNow();
}

/* =====================================================================
   P2 计时器（显示交给悬浮球窗口）
   ===================================================================== */
function emitClipState() {
  const row = rowById(state.timerTaskId) || (state.selectedId ? rowById(state.selectedId) : null);
  api.emit('clip-state', {
    running: state.timerRunning,
    elapsed: state.seconds,
    taskId: row ? row.id : null,
    proj: row ? row.proj : '',
    task: row ? row.task : '',
  });
}
function setClipIdle() {
  state.timerRunning = false;
  state.timerTaskId = null;
  state.seconds = 0;
  emitClipState();
}
function setClipTiming(row) {
  state.timerRunning = true;
  state.timerTaskId = row.id;
  emitClipState();
}
function hint(msg) {
  toast(msg);
  api.emit('clip-toast', msg);
}
function toggleTimer() {
  if (guardModal()) return;
  if (state.detailDirty) { toast('有未保存的修改，请先保存或取消'); return; }
  if (state.timerRunning) stopTimer();
  else startTimer();
}
async function startTimer() {
  if (!state.selectedId) { hint('请先选择项目-任务'); return; }
  const d = rowById(state.selectedId);
  if (!d) { hint('请先选择项目-任务'); return; }
  closeAllSheets();
  try { await api.startTimer(d.projectId, d.taskId); }
  catch (err) { hint(String(err)); return; }
  state.seconds = 0;
  setClipTiming(d);
}
async function stopTimer(manual = true) {
  if (!state.timerRunning) return;
  let pending;
  try { pending = await api.stopTimer(); }
  catch (err) { hint(String(err)); return; }
  closeRemind();
  setClipIdle();
  if (manual) closeAllSheets();
  if (pending) await presentF3(pending);
}
api.onTick(sec => {
  if (!state.timerRunning) return;
  state.seconds = sec;
});

/* =====================================================================
   P5-1 数据表：筛选 / 排序 / 渲染
   ===================================================================== */
const scrollEl = $('#tableScroll');
scrollEl.addEventListener('wheel', e => { if (anySheetOpen()) e.preventDefault(); }, { passive: false });
scrollEl.addEventListener('touchmove', e => { if (anySheetOpen()) e.preventDefault(); }, { passive: false });
const sheetObserver = new MutationObserver(() => {
  const open = anySheetOpen();
  scrollEl.classList.toggle('is-locked', open);
  scrollEl.classList.toggle('is-sheet-open', open);
});
$$('.sheet').forEach(s => sheetObserver.observe(s, { attributes: true, attributeFilter: ['class'] }));

const FILTER_DEFS = {
  proj: { state: 'proj', opts: () => ['全部', ...projectNames()] },
  pri: { state: 'pri', opts: () => ['全部', '高', '中', '低'] }
};

function filtered() {
  let list = DATA.slice();
  if (state.proj !== '全部') list = list.filter(d => d.proj === state.proj);
  if (state.pri !== '全部') list = list.filter(d => d.pri === state.pri);
  if (state.status !== '全部') list = list.filter(d => d.status === state.status);
  if (state.sortKey === 'ddl') {
    list.sort((a, b) => {
      const av = ddlDays(a.ddl), bv = ddlDays(b.ddl);
      if (av === null && bv === null) return 0;
      if (av === null) return 1;         // 无截止日期始终排最后
      if (bv === null) return -1;
      return (av - bv) * state.sortDir;
    });
  } else {
    list.sort((a, b) => b.lastOp - a.lastOp);
  }
  return list;
}
function renderTable() {
  const list = filtered();
  if (state.selectedId && !list.some(d => d.id === state.selectedId)) state.selectedId = null;
  const top = state.showNewRow
    ? '<tr class="blank blank--new"><td></td><td></td><td></td><td></td></tr>'
    : '';
  const html = list.map(d => {
    const sel = d.id === state.selectedId ? 'selected' : '';
    const strip = d.status === '已完成' ? '#8e8e93' : 'var(--accent)';
    return `<tr data-id="${esc(d.id)}" class="${sel}" style="--strip:${strip}">
      <td title="${esc(d.proj)}">${esc(d.proj)}</td>
      <td title="${esc(d.task)}">${esc(d.task)}</td>
      <td>${esc(ddlText(d.ddl))}</td>
      <td><span class="badge pri-${esc(d.pri)}">${esc(d.pri)}</span></td></tr>`;
  }).join('');
  let bot = '';
  for (let i = 0; i < 6; i++) bot += '<tr class="blank blk"><td></td><td></td><td></td><td></td></tr>';
  $('#tbody').innerHTML = top + html + bot;
  updateHeaderMarks();
}
function updateHeaderMarks() {
  $$('thead th[data-filter]').forEach(th => {
    const cur = state[FILTER_DEFS[th.dataset.filter].state];
    th.classList.toggle('is-active', cur !== '全部');
  });
  const s = $('thead th[data-sort="ddl"]');
  if (state.sortKey === 'ddl') {
    s.classList.add('is-active');
    s.classList.toggle('is-asc', state.sortDir === 1);
    s.classList.toggle('is-desc', state.sortDir === -1);
  } else {
    s.classList.remove('is-active', 'is-asc', 'is-desc');
  }
}

/* 表头筛选下拉 */
const hdrMenu = $('#hdrMenu');
function closeHeaderMenu() { hdrMenu.classList.remove('open'); }
function openHeaderMenu(th, key) {
  const def = FILTER_DEFS[key];
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  const cur = state[def.state];
  hdrMenu.innerHTML = def.opts().map(o => {
    const on = (o === cur);
    return `<div class="mi ${on ? 'active' : ''}" data-v="${esc(o)}"><span class="chk">${on ? '✓' : ''}</span>${esc(o)}</div>`;
  }).join('');
  hdrMenu.style.top = headerH + 'px';
  hdrMenu.classList.add('open');
  const maxLeft = Math.max(0, scrollEl.clientWidth - hdrMenu.offsetWidth - 4);
  hdrMenu.style.left = Math.min(Math.max(0, th.offsetLeft), maxLeft) + 'px';
  hdrMenu.querySelectorAll('.mi').forEach(mi => {
    mi.onclick = ev => {
      ev.stopPropagation();
      state[def.state] = mi.dataset.v;
      closeHeaderMenu();
      renderTable();
    };
  });
}
$$('thead th[data-filter]').forEach(th => {
  th.addEventListener('click', e => {
    e.stopPropagation();
    if (guardModal()) return;
    if (hdrMenu.classList.contains('open')) { closeHeaderMenu(); return; }
    openHeaderMenu(th, th.dataset.filter);
  });
});
$('thead th[data-sort="ddl"]').addEventListener('click', e => {
  e.stopPropagation();
  if (guardModal()) return;
  const cur = state.sortKey === 'ddl' ? state.sortDir : 0;
  if (cur === 0) { state.sortKey = 'ddl'; state.sortDir = 1; }
  else if (cur === 1) { state.sortDir = -1; }
  else { state.sortKey = ''; state.sortDir = 0; }
  renderTable();
});
scrollEl.addEventListener('scroll', closeHeaderMenu);
document.addEventListener('click', e => {
  if (!e.target.closest('.hdr-menu') && !e.target.closest('th[data-filter]')) closeHeaderMenu();
});

/* 滚动钳制：常规浏览最多滚到「底部两个空行」 */
function maxScrollFor2Blanks() {
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  const visibleBody = scrollEl.clientHeight - headerH;
  const blanks = $('#tbody').querySelectorAll('tr.blank.blk');
  const second = blanks[1];
  if (!second) return scrollEl.scrollHeight;
  return second.offsetTop + second.offsetHeight - headerH - visibleBody;
}
scrollEl.addEventListener('scroll', () => {
  if (!state.clampEnabled) return;
  const max = maxScrollFor2Blanks();
  if (scrollEl.scrollTop > max) scrollEl.scrollTop = max;
});

/* =====================================================================
   表格行交互：单击选中 / 再次单击或双击展开详情
   ===================================================================== */
function selectRowVisual(tr) {
  $$('#tbody tr.selected').forEach(x => x.classList.remove('selected'));
  if (tr) tr.classList.add('selected');
}
function guardModal() {
  if (state.modalLocked) {
    toast('请先保存本次阶段记录');
    api.emit('clip-toast', '请先保存本次阶段记录');
    return true;
  }
  return false;
}
$('#tbody').addEventListener('click', e => {
  const tr = e.target.closest('tr');
  if (!tr || tr.classList.contains('blank')) return;
  if (guardModal()) return;
  const id = tr.dataset.id;
  if (state.timerRunning) {
    if (id !== String(state.timerTaskId)) { toast('计时中，无法切换项目'); return; }
    if (state.selectedId === id) openDetail(id);
    return;
  }
  if (state.selectedId === id) { openDetail(id); return; }
  state.selectedId = id;
  selectRowVisual(tr);
});
$('#tbody').addEventListener('dblclick', e => {
  const tr = e.target.closest('tr');
  if (!tr || tr.classList.contains('blank')) return;
  if (guardModal()) return;
  const id = tr.dataset.id;
  if (state.timerRunning && id !== String(state.timerTaskId)) { toast('计时中，无法切换项目'); return; }
  openDetail(id);
});

/* =====================================================================
   数据表右键菜单（F4：导出 Excel / CSV、查看数据库、删除任务）
   ===================================================================== */
const ctxMenu = $('#ctxMenu');
let ctxTarget = null;
function closeCtxMenu() { ctxMenu.classList.remove('open'); ctxTarget = null; }
function openCtxMenu(x, y, id) {
  const row = rowById(id);
  if (!row) return;
  ctxTarget = id;
  const locked = state.timerRunning;
  ctxMenu.innerHTML = `
    <div class="mi" data-act="csv">导出 CSV（当前全部任务）</div>
    <div class="mi" data-act="xlsx">导出 Excel（当前全部任务）</div>
    <div class="sep"></div>
    <div class="mi" data-act="db">打开数据库所在目录</div>
    <div class="sep"></div>
    <div class="mi is-danger ${locked ? 'is-disabled' : ''}" data-act="del">删除「${esc(row.proj)} · ${esc(row.task)}」</div>`;
  ctxMenu.classList.add('open');
  const w = ctxMenu.offsetWidth, h = ctxMenu.offsetHeight;
  ctxMenu.style.left = Math.max(6, Math.min(x, window.innerWidth - w - 6)) + 'px';
  ctxMenu.style.top = Math.max(6, Math.min(y, window.innerHeight - h - 6)) + 'px';
}
$('#tbody').addEventListener('contextmenu', e => {
  const tr = e.target.closest('tr');
  if (!tr || tr.classList.contains('blank')) return;
  e.preventDefault();
  if (anySheetOpen() || state.modalLocked) return;
  openCtxMenu(e.clientX, e.clientY, tr.dataset.id);
});
ctxMenu.addEventListener('click', async e => {
  const item = e.target.closest('.mi');
  if (!item) return;
  if (item.classList.contains('is-disabled')) { toast('计时中，无法删除任务'); return; }
  const act = item.dataset.act;
  const id = ctxTarget;
  const row = rowById(id);
  closeCtxMenu();
  if (!row) return;
  if (act === 'csv' || act === 'xlsx') {
    try {
      const path = await api.exportTasks(act);
      const name = String(path).split(/[\\/]/).pop();
      toast('已导出：' + name);
    } catch (err) { toast(String(err)); }
  } else if (act === 'db') {
    try { await api.openDataDir(); } catch (err) { toast(String(err)); }
  } else if (act === 'del') {
    if (!window.confirm(`删除「${row.proj} · ${row.task}」及其全部阶段记录？此操作不可撤销。`)) return;
    try { await api.deleteTask(row.projectId, row.taskId); }
    catch (err) { toast(String(err)); return; }
    if (state.selectedId === id) state.selectedId = null;
    state.summaryStale = true;
    await reloadRows();
    toast('已删除任务');
  }
});
document.addEventListener('click', e => { if (!e.target.closest('#ctxMenu')) closeCtxMenu(); });
document.addEventListener('keydown', e => { if (e.key === 'Escape') closeCtxMenu(); });
window.addEventListener('resize', closeCtxMenu);
scrollEl.addEventListener('scroll', closeCtxMenu);

/* =====================================================================
   上滑面板通用
   ===================================================================== */
function closeAllSheets() {
  ['sheet-detail', 'sheet-new', 'sheet-f3', 'sheet-settings'].forEach(id => $('#' + id).classList.remove('open'));
}
function anySheetOpen() { return !!document.querySelector('.sheet.open'); }
function rememberScroll() { state.savedScrollTop = scrollEl.scrollTop; }
function restoreScroll() {
  $('#tableSpacer').style.height = '0px';
  scrollEl.scrollTop = state.savedScrollTop || 0;
}
function ensureTaskTab() {
  $$('.tab').forEach(x => x.classList.toggle('active', x.dataset.tab === 'task'));
  $$('.tabpane').forEach(x => x.classList.toggle('active', x.id === 'pane-task'));
  $('#newBtn').classList.remove('hidden');
}
function sheetTopForRow(row) {
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  return headerH + (row ? row.offsetHeight : 40);
}
function ensureScrollRoom(row) {
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  const spacer = $('#tableSpacer');
  spacer.style.height = '0px';
  const need = Math.max(0, row.offsetTop - headerH);
  const shortfall = need + scrollEl.clientHeight - scrollEl.scrollHeight;
  spacer.style.height = Math.max(0, shortfall + 2) + 'px';
  return need;
}
function closeAndRestoreSheets() {
  const hadNew = state.showNewRow;
  state.showNewRow = false;
  closeAllSheets();
  state.clampEnabled = true;
  state.detailDirty = false;
  state.editingId = null;
  state.detailSnapshot = null;
  if (hadNew) renderTable();
  restoreScroll();
}

/* =====================================================================
   P5-2 项目详细信息
   ===================================================================== */
/* 项目名：可输入 + 仅点击右侧箭头弹出下拉 */
function buildComboList(listEl, inputEl) {
  const names = projectNames();
  listEl.innerHTML = names.length
    ? names.map(n => `<div class="ci" data-v="${esc(n)}">${esc(n)}</div>`).join('')
    : '<div class="ci empty">暂无已有项目，可直接输入新建</div>';
  listEl.querySelectorAll('.ci[data-v]').forEach(ci => {
    ci.onclick = ev => {
      ev.stopPropagation();
      inputEl.value = ci.dataset.v;
      ci.closest('.combo').classList.remove('open');
      inputEl.dispatchEvent(new Event('input', { bubbles: true }));
    };
  });
}
function closeCombos() { $$('.combo.open').forEach(c => c.classList.remove('open')); }
function toggleCombo(btn) {
  const combo = btn.closest('.combo');
  const input = combo.querySelector('input');
  const list = combo.querySelector('.combo-list');
  const wasOpen = combo.classList.contains('open');
  closeCombos();
  if (wasOpen) return;
  buildComboList(list, input);
  combo.classList.add('open');
}
$$('.combo-arrow').forEach(b => {
  b.addEventListener('click', e => { e.stopPropagation(); toggleCombo(b); });
});
document.addEventListener('click', e => {
  if (e.target.closest('.combo-arrow') || e.target.closest('.combo-list')) return;
  closeCombos();
});
/* 自定义字段：每个字段完整占一行，显示在备注下方 */
function customValues(containerId) {
  const o = {};
  $$('#' + containerId + ' input[data-cf]').forEach(i => { o[i.dataset.cf] = i.value; });
  return o;
}
function renderCustomFields(containerId, values) {
  const box = $('#' + containerId);
  if (!box) return;
  if (!state.customFields.length) { box.innerHTML = ''; return; }
  box.innerHTML = state.customFields.map(name => {
    const raw = (values && values[name] != null) ? String(values[name]) : '';
    return `<div class="fld full" style="margin-top:10px"><label>${esc(name)}</label>
      <input type="text" data-cf="${esc(name)}" value="${esc(raw)}" placeholder="选填"></div>`;
  }).join('');
}
function markDetailDirty() {
  if (state.loadingForm) return;
  if (state.detailDirty) return;
  state.detailDirty = true;
  $('#detailCancel').disabled = false;
  $('#detailSave').disabled = false;
  $('#detailHint').textContent = '有未保存的修改';
}
function loadDetailForm(item) {
  state.loadingForm = true;
  $('#dProj').value = item.proj || '';
  $('#dTask').value = item.task || '';
  $('#dDDL').value = item.ddl || '';
  $('#dNote').value = item.note || '';
  renderCustomFields('dCustomFields', item.custom);
  fillTri('#dPri', item.pri);
  fillTri('#sdStatus', item.status);
  renderDetailStages(item);
  const idx = item.stages.length ? item.stages.length - 1 : null;
  selectStage(idx);
  state.loadingForm = false;
  state.detailDirty = false;
  $('#detailCancel').disabled = true;
  $('#detailSave').disabled = true;
  $('#detailHint').textContent = '仅在有改动时可操作';
}
function renderDetailStages(item) {
  const body = $('#dStageBody');
  $('#dStageCount').textContent = `共 ${item.stages.length} 段`;
  if (!item.stages.length) {
    body.innerHTML = '<tr><td colspan="3" style="color:var(--t3);font-size:0.688rem;padding:8px 6px">暂无阶段记录（开始计时后自动生成）</td></tr>';
    return;
  }
  body.innerHTML = item.stages.map((s, i) =>
    `<tr class="stage-row" data-idx="${i}"><td>阶段 ${esc(s.no)}</td><td>${esc(s.date)}</td><td title="${esc(s.out || '')}">${esc(s.out || '—')}</td></tr>`
  ).join('');
  body.querySelectorAll('tr.stage-row').forEach(tr => {
    tr.onclick = () => {
      commitStageDraft();
      selectStage(+tr.dataset.idx);
    };
  });
  body.querySelectorAll('tr.stage-row').forEach(tr => {
    tr.classList.toggle('sel', +tr.dataset.idx === state.editingStage);
  });
}
/* 切换阶段前：把当前阶段的编辑内容写回内存（保存前不落库），保证切换不丢内容 */
function commitStageDraft() {
  const item = rowById(state.editingId);
  const idx = state.editingStage;
  if (!item || idx === null || idx === undefined || !item.stages[idx]) return;
  const s = item.stages[idx];
  const out = $('#sdOut').value;
  const next = $('#sdNext').value;
  if (s.out !== out || s.next !== next) {
    s.out = out; s.next = next;
    markDetailDirty();
    renderDetailStages(item);
    selectStage(idx);
  }
}
function selectStage(idx) {
  const item = rowById(state.editingId);
  if (!item) return;
  state.editingStage = idx;
  $$('#dStageBody tr.stage-row').forEach(tr => tr.classList.toggle('sel', +tr.dataset.idx === idx));
  const s = (idx === null || idx === undefined) ? null : item.stages[idx];
  if (!s) {
    $('#sdNo').textContent = '—';
    $('#sdDur').textContent = '—';
    $('#sdOut').value = '';
    $('#sdNext').value = '';
    $('#sdOut').disabled = true;
    $('#sdNext').disabled = true;
    $$('#sdStatus button').forEach(b => b.disabled = true);
    return;
  }
  state.loadingForm = true;
  $('#sdNo').textContent = '阶段 ' + s.no;
  $('#sdDur').textContent = fmtDur(s.durSec);
  $('#sdOut').value = s.out || '';
  $('#sdNext').value = s.next || '';
  state.loadingForm = false;
  $('#sdOut').disabled = false;
  $('#sdNext').disabled = false;
  $$('#sdStatus button').forEach(b => b.disabled = false);
}

async function openDetail(id) {
  if (guardModal()) return;
  const already = $('#sheet-detail').classList.contains('open') && state.editingId === id;
  if (already) return;
  if ($('#sheet-detail').classList.contains('open') && state.detailDirty) {
    toast('有未保存的修改，请先保存或取消'); return;
  }
  const srcRow = rowById(id);
  if (!srcRow) return;
  let item;
  try { item = await api.getTask(srcRow.projectId, srcRow.taskId); }
  catch (err) { toast(String(err)); return; }
  const i = DATA.findIndex(x => x.id === id);
  if (i >= 0) DATA[i] = item;
  state.detailSnapshot = JSON.stringify(item);
  rememberScroll();
  closeAllSheets();
  await openBoardNow();
  ensureTaskTab();
  state.clampEnabled = false;
  state.showNewRow = false;
  state.selectedId = id;
  state.editingId = id;
  closeHeaderMenu();
  renderTable();
  const row = $('#tbody').querySelector(`tr[data-id="${CSS.escape(String(id))}"]`);
  if (row) scrollEl.scrollTop = ensureScrollRoom(row);
  loadDetailForm(item);
  $('#sheet-detail').style.top = sheetTopForRow(row) + 'px';
  $('#sheet-detail').classList.add('open');
}
function closeDetailAndRestore(keepChanges) {
  if (!keepChanges && state.detailSnapshot) {
    const item = rowById(state.editingId);
    if (item) Object.assign(item, JSON.parse(state.detailSnapshot));
  }
  state.detailSnapshot = null;
  state.detailDirty = false;
  state.editingId = null;
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
}
$('#detailClose').addEventListener('click', () => {
  if (state.detailDirty) { toast('有未保存的修改，请先保存或取消'); return; }
  closeDetailAndRestore(true);
});
$('#detailCancel').addEventListener('click', () => closeDetailAndRestore(false));
$('#detailSave').addEventListener('click', async () => {
  const item = rowById(state.editingId);
  if (!item) return;
  commitStageDraft();
  const proj = $('#dProj').value.trim();
  if (!proj) { toast('请填写项目名'); return; }
  const status = activeTri('#sdStatus') || item.status;
  const payload = {
    projectName: proj,
    taskName: $('#dTask').value.trim(),
    dueDate: $('#dDDL').value || null,
    priority: activeTri('#dPri') || item.pri,
    status,
    remark: $('#dNote').value,
    customFields: customValues('dCustomFields'),
  };
  try {
    await api.updateTask(item.projectId, item.taskId, payload);
    /* 阶段改动逐条落库（与打开时的快照比对） */
    const original = state.detailSnapshot ? JSON.parse(state.detailSnapshot) : null;
    if (original) {
      for (const s of item.stages) {
        const o = original.stages.find(x => x.no === s.no);
        if (o && (o.out !== s.out || o.next !== s.next)) {
          await api.updateStage(item.projectId, item.taskId, s.no, {
            result: s.out, nextGoal: s.next, status: o.status || status
          });
        }
      }
    }
  } catch (err) { toast(String(err)); return; }
  closeDetailAndRestore(true);
  state.summaryStale = true;
  await reloadRows();
  toast('已保存修改');
});
['dProj', 'dTask', 'dDDL', 'dNote', 'sdOut', 'sdNext'].forEach(id => {
  $('#' + id).addEventListener('input', markDetailDirty);
  $('#' + id).addEventListener('change', markDetailDirty);
});
$('#dCustomFields').addEventListener('input', markDetailDirty);
$('#dPri').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  fillTri('#dPri', b.dataset.v);
  markDetailDirty();
});
$('#sdStatus').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  fillTri('#sdStatus', b.dataset.v);
  commitStageDraft();
  markDetailDirty();
});

/* =====================================================================
   P7 新建项目 - 任务
   ===================================================================== */
function resetNewForm() {
  $('#nProj').value = '';
  $('#nTask').value = '';
  $('#nDDL').value = '';
  $('#nNote').value = '';
  fillTri('#nPri', '中');
  renderCustomFields('nCustomFields', null);
  closeCombos();
}
async function openNewSheet() {
  if (guardModal()) return;
  if (state.detailDirty) { toast('有未保存的修改，请先保存或取消'); return; }
  if ($('#sheet-new').classList.contains('open')) { closeNewSheet(); return; }
  rememberScroll();
  closeAllSheets();
  await openBoardNow();
  ensureTaskTab();
  state.clampEnabled = false;
  state.showNewRow = true;
  renderCustomFields('nCustomFields', customValues('nCustomFields'));
  renderTable();
  scrollEl.scrollTop = 0;
  const row = $('#tbody').querySelector('tr.blank--new');
  $('#sheet-new').style.top = sheetTopForRow(row) + 'px';
  $('#sheet-new').classList.add('open');
  setTimeout(() => $('#nTask').focus(), 120);
}
function closeNewSheet() {
  state.showNewRow = false;
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
}
$('#nPri').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  fillTri('#nPri', b.dataset.v);
});
$('#newBtn').addEventListener('click', e => { e.stopPropagation(); openNewSheet(); });
$('#newClose').addEventListener('click', closeNewSheet);
$('#newCancel').addEventListener('click', () => {
  resetNewForm();
  closeNewSheet();
});
$('#newSave').addEventListener('click', async () => {
  const proj = $('#nProj').value.trim();
  const task = $('#nTask').value.trim();
  if (!proj) { toast('请填写项目名'); return; }
  if (!task) { toast('请填写任务名'); return; }
  let row;
  try {
    row = await api.createTask({
      projectName: proj, taskName: task,
      dueDate: $('#nDDL').value || null,
      priority: activeTri('#nPri') || '中',
      status: '未完成',
      remark: $('#nNote').value,
      customFields: customValues('nCustomFields'),
    });
  } catch (err) { toast(String(err)); return; }
  state.selectedId = row.id;
  state.showNewRow = false;
  resetNewForm();
  closeAllSheets();
  state.clampEnabled = true;
  state.summaryStale = true;
  await reloadRows();
  scrollEl.scrollTop = 0;
  toast(`已保存：P${String(row.projectId).padStart(3, '0')}-T${String(row.taskId).padStart(2, '0')}`);
});

/* =====================================================================
   P8 设置
   ===================================================================== */
function fillTimeSelect(el, def) {
  if (!el) return;
  let html = '';
  for (let m = 0; m < 24 * 60; m += 30) {
    const v = pad(Math.floor(m / 60)) + ':' + pad(m % 60);
    html += `<option value="${v}"${v === def ? ' selected' : ''}>${v}</option>`;
  }
  el.innerHTML = html;
}
function setSelectValue(el, v) {
  if (!el || !v) return;
  if (![...el.options].some(o => o.value === v)) {
    const o = document.createElement('option');
    o.value = v; o.textContent = v;
    el.appendChild(o);
  }
  el.value = v;
}
function fillSettingsForm() {
  const cfg = state.config || {};
  applyTheme(cfg.appearance?.theme || 'dark', { silent: true });
  const fs = Math.min(20, Math.max(14, parseInt(cfg.appearance?.fontSize, 10) || 16));
  $('#fsRange').value = fs;
  applyFontSize(fs);
  setSelectValue($('#setStart'), cfg.reminder?.workStart || '09:00');
  setSelectValue($('#setEnd'), cfg.reminder?.workEnd || '18:00');
  $('#setX').value = cfg.reminder?.limitXMinutes || 60;
  $('#setY').value = cfg.reminder?.stopYSeconds || 60;
  $('#setBase').value = cfg.llm?.baseUrl || 'https://api.openai.com/v1';
  $('#setKey').value = cfg.llm?.apiKey || '';
  $('#setModel').value = cfg.llm?.model || 'gpt-4o-mini';
  $('#setTemplatePath').value = state.appInfo?.templatePath || '（未加载）';
  $('#setSkillPath').value = state.appInfo?.skillPath || '（未加载）';
  renderFieldList();
}
async function openSettings() {
  if (guardModal()) return;
  if (state.detailDirty) { toast('有未保存的修改，请先保存或取消'); return; }
  if ($('#sheet-settings').classList.contains('open')) { closeAndRestoreSheets(); return; }
  rememberScroll();
  closeAllSheets();
  closeHeaderMenu();
  state.showNewRow = false;
  await openBoardNow();
  state.clampEnabled = false;
  fillSettingsForm();
  renderTable();
  $('#sheet-settings').classList.add('open');
}
$('#setBtn').addEventListener('click', e => { e.stopPropagation(); openSettings(); });
$('#setClose').addEventListener('click', closeAndRestoreSheets);
$('#setSave').addEventListener('click', async () => {
  const start = $('#setStart').value;
  const end = $('#setEnd').value;
  if (start >= end) { toast('上班时间需早于下班时间'); return; }
  const cfg = JSON.parse(JSON.stringify(state.config || {}));
  cfg.llm = {
    baseUrl: $('#setBase').value.trim() || 'https://api.openai.com/v1',
    apiKey: $('#setKey').value.trim(),
    model: $('#setModel').value.trim() || 'gpt-4o-mini',
  };
  cfg.appearance = { theme: state.themePref, fontSize: parseInt($('#fsRange').value, 10) || 16 };
  cfg.reminder = {
    workStart: start, workEnd: end,
    limitXMinutes: Math.max(1, parseInt($('#setX').value, 10) || 60),
    stopYSeconds: Math.max(5, parseInt($('#setY').value, 10) || 60),
  };
  cfg.customFields = state.customFields.slice();
  try { await api.saveConfig(cfg); }
  catch (err) { toast(String(err)); return; }
  state.config = cfg;
  closeAndRestoreSheets();
  toast('设置已保存');
});
$('#setFieldAdd').addEventListener('click', () => addCustomField($('#setField').value));
$('#setField').addEventListener('keydown', e => {
  if (e.key === 'Enter') { e.preventDefault(); addCustomField($('#setField').value); }
});
async function addCustomField(value) {
  const v = String(value || '').trim();
  if (!v) { toast('请输入字段名'); return; }
  if (state.customFields.includes(v)) { toast('该字段已存在'); return; }
  state.customFields.push(v);
  $('#setField').value = '';
  renderFieldList();
  syncCustomFieldInputs();
  await persistCustomFields();
  toast('已添加自定义字段：' + v);
}
async function persistCustomFields() {
  if (!state.config) return;
  state.config.customFields = state.customFields.slice();
  try { await api.saveConfig(state.config); } catch (err) { toast(String(err)); }
}
function renderFieldList() {
  const box = $('#setFieldList');
  if (!state.customFields.length) { box.innerHTML = '<span class="note">暂无自定义字段</span>'; return; }
  box.innerHTML = state.customFields.map((n, i) =>
    `<span class="field-chip">${esc(n)}<button type="button" data-i="${i}" title="删除">×</button></span>`
  ).join('');
  box.querySelectorAll('button[data-i]').forEach(b => {
    b.onclick = async () => {
      state.customFields.splice(+b.dataset.i, 1);
      renderFieldList();
      syncCustomFieldInputs();
      await persistCustomFields();
    };
  });
}
function syncCustomFieldInputs() {
  renderCustomFields('nCustomFields', customValues('nCustomFields'));
  if ($('#sheet-detail').classList.contains('open')) renderCustomFields('dCustomFields', customValues('dCustomFields'));
}

/* 打开 / 恢复默认报告配置 */
async function openConfigFile(rel, label) {
  try { await api.openConfigFile(rel); }
  catch (err) { toast(String(err)); }
}
$('#btnOpenTemplate').addEventListener('click', () => openConfigFile(state.config?.report?.templateFile || 'config/report-template.yaml'));
$('#btnOpenSkill').addEventListener('click', () => openConfigFile(state.config?.report?.skillFile || 'config/report-skill.md'));
$('#btnResetTemplate').addEventListener('click', () => resetConfigFile('template', '模板'));
$('#btnResetSkill').addEventListener('click', () => resetConfigFile('skill', 'Skill'));
async function resetConfigFile(kind, label) {
  if (!window.confirm(`确定把报告${label}文件恢复为随包默认内容吗？当前修改会丢失。`)) return;
  try { await api.resetReportConfig(kind); }
  catch (err) { toast(String(err)); return; }
  toast(`已恢复默认${label}文件`);
  state.summaryStale = true;
}

/* 外观：深色 / 浅色 / 跟随系统 */
const mqLight = window.matchMedia('(prefers-color-scheme: light)');
function resolveTheme(pref) {
  return pref === 'auto' ? (mqLight.matches ? 'light' : 'dark') : pref;
}
let appearanceSaveTimer = null;
function emitAppearance() {
  api.emit('clip-appearance', {
    theme: state.themePref,
    fontSize: parseInt($('#fsRange').value, 10) || 16,
  });
}
function applyTheme(pref, opts) {
  state.themePref = pref || 'dark';
  document.documentElement.setAttribute('data-theme', resolveTheme(state.themePref));
  $$('#themeSeg button').forEach(b => b.classList.toggle('active', b.dataset.v === state.themePref));
  emitAppearance();
  if (state.lastSummary) renderSummary(state.lastSummary);
  if (!(opts && opts.silent) && state.config) {
    state.config.appearance = {
      theme: state.themePref,
      fontSize: parseInt($('#fsRange').value, 10) || 16,
    };
    clearTimeout(appearanceSaveTimer);
    appearanceSaveTimer = setTimeout(() => {
      if (state.config) api.saveConfig(state.config).catch(() => {});
    }, 300);
  }
}
const fsRange = $('#fsRange');
function applyFontSize(px) {
  const v = Math.min(20, Math.max(14, parseInt(px, 10) || 16));
  document.documentElement.style.setProperty('--ui-fs', v + 'px');
  $('#fsVal').textContent = v + 'px';
  emitAppearance();
}
fsRange.addEventListener('input', () => applyFontSize(fsRange.value));
fsRange.addEventListener('change', () => {
  if (!state.config) return;
  state.config.appearance = { theme: state.themePref, fontSize: parseInt(fsRange.value, 10) || 16 };
  api.saveConfig(state.config).catch(() => {});
});
$('#themeSeg').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  applyTheme(b.dataset.v);
  toast('外观：' + ({ dark: '深色', light: '浅色', auto: '跟随系统' })[b.dataset.v]);
});
if (mqLight.addEventListener) {
  mqLight.addEventListener('change', () => { if (state.themePref === 'auto') applyTheme('auto'); });
}

/* =====================================================================
   外部点击拦截（P4）：只关闭、不触发底层；模态下完全拦截
   ===================================================================== */
function blockOutside(e) {
  if (!anySheetOpen()) return;
  if (e.target.closest('.sheet')) return;
  if (e.target.closest('.tool-ic')) return;
  if (state.modalLocked) {
    if (e.type === 'click') toast('请先保存本次阶段记录');
    e.stopPropagation(); e.preventDefault();
    return;
  }
  if ($('#sheet-detail').classList.contains('open') && state.detailDirty) {
    if (e.type === 'click') toast('有未保存的修改，请先保存或取消');
    e.stopPropagation(); e.preventDefault();
    return;
  }
  e.stopPropagation(); e.preventDefault();
  if (e.type === 'click') closeAndRestoreSheets();
}
$('#board').addEventListener('mousedown', blockOutside, true);
$('#board').addEventListener('click', blockOutside, true);

/* =====================================================================
   标签页切换
   ===================================================================== */
$$('.tab').forEach(t => t.addEventListener('click', () => {
  if (guardModal()) return;
  if (state.detailDirty) { toast('有未保存的修改，请先保存或取消'); return; }
  closeAllSheets();
  state.showNewRow = false;
  state.clampEnabled = true;
  $('#tableSpacer').style.height = '0px';
  $$('.tab').forEach(x => x.classList.toggle('active', x === t));
  $$('.tabpane').forEach(x => x.classList.toggle('active', x.id === 'pane-' + t.dataset.tab));
  $('#newBtn').classList.toggle('hidden', t.dataset.tab === 'summary');
  renderTable();
  if (t.dataset.tab === 'summary') loadSummary();
}));

/* =====================================================================
   P5-3 本次阶段记录（停止计时后 · 模态）
   ===================================================================== */
function renderF3Stages(item, pendingNo, date) {
  const body = $('#f3StageBody');
  let html = item.stages.map(s =>
    `<tr><td>阶段 ${esc(s.no)}</td><td>${esc(s.date)}</td><td>${esc(s.out || '—')}</td></tr>`
  ).join('');
  html += `<tr class="stage-row sel"><td>阶段 ${esc(pendingNo)}</td><td>${esc(date)}</td><td>本次 · 保存后生成</td></tr>`;
  body.innerHTML = html;
  $('#f3StageHint').textContent = `已有 ${item.stages.length} 段 + 本次`;
}
async function presentF3(pending) {
  const id = pending.projectId + '-' + pending.taskId;
  const item = rowById(id);
  if (!item) { toast('任务不存在，已丢弃本次阶段记录'); api.discardPending().catch(() => {}); setClipIdle(); return; }
  state.modalLocked = true;
  state.pendingStage = pending;
  rememberScroll();
  closeRemind();
  closeAllSheets();
  await openBoardNow();
  ensureTaskTab();
  state.clampEnabled = false;
  state.showNewRow = false;
  state.selectedId = id;
  closeHeaderMenu();
  renderTable();
  const row = $('#tbody').querySelector(`tr[data-id="${CSS.escape(String(id))}"]`);
  if (row) scrollEl.scrollTop = ensureScrollRoom(row);
  const pendingNo = item.stages.length + 1;
  $('#f3Task').textContent = item.proj + ' · ' + item.task;
  $('#f3DDL').textContent = ddlText(item.ddl);
  $('#f3Pri').textContent = item.pri;
  $('#f3Note').textContent = item.note || '—';
  renderF3Stages(item, pendingNo, (pending.endedAt || todayStr()).slice(0, 10));
  $('#f3No').textContent = '阶段 ' + pendingNo;
  $('#f3Dur').textContent = fmtDur(pending.duration || state.seconds);
  const last = item.stages[item.stages.length - 1];
  $('#f3Out').value = last ? (last.next || '') : '';
  $('#f3Next').value = '';
  fillTri('#f3Status', '未完成');
  $('#sheet-f3').style.top = sheetTopForRow(row) + 'px';
  $('#sheet-f3').classList.add('open');
  setTimeout(() => { try { $('#f3Out').focus(); } catch (_) {} }, 150);
}
$('#f3Status').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  fillTri('#f3Status', b.dataset.v);
});
$('#f3Save').addEventListener('click', async () => {
  const p = state.pendingStage;
  if (!p) { toast('没有待保存的阶段记录'); return; }
  const no = (rowById(p.projectId + '-' + p.taskId)?.stages.length || 0) + 1;
  try {
    await api.addStage({
      projectId: p.projectId, taskId: p.taskId,
      startedAt: p.startedAt, endedAt: p.endedAt, duration: p.duration,
      result: $('#f3Out').value,
      nextGoal: $('#f3Next').value,
      status: activeTri('#f3Status') || '未完成',
    });
  } catch (err) { toast(String(err)); return; }
  try { await api.discardPending(); } catch (_) {}
  state.modalLocked = false;
  state.pendingStage = null;
  state.seconds = 0;
  closeAllSheets();
  state.clampEnabled = true;
  state.summaryStale = true;
  await reloadRows();
  restoreScroll();
  toast('已保存阶段 ' + no);
});
$('#f3Delete').addEventListener('click', async () => {
  state.modalLocked = false;
  state.pendingStage = null;
  state.seconds = 0;
  try { await api.discardPending(); } catch (_) {}
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
  toast('已删除本次阶段记录（不计入累计）');
});

/* =====================================================================
   P9 计时提醒窗（防遗忘）
   ===================================================================== */
let remindTimer = null;
async function openRemind(seconds) {
  if (state.modalLocked) return;
  await openBoardNow();
  closeRemindTimerOnly();
  let n = Math.max(0, parseInt(seconds, 10) || 60);
  $('#r9count').textContent = n;
  $('#remind9').classList.add('open');
  remindTimer = setInterval(() => {
    n = Math.max(0, n - 1);
    $('#r9count').textContent = n;
    if (n <= 0) closeRemindTimerOnly();
  }, 1000);
}
function closeRemindTimerOnly() {
  clearInterval(remindTimer); remindTimer = null;
}
function closeRemind() {
  closeRemindTimerOnly();
  $('#remind9').classList.remove('open');
}
api.onReminderShow(payload => openRemind(payload && payload.seconds ? payload.seconds : 60));
api.onReminderTick(sec => {
  if ($('#remind9').classList.contains('open')) $('#r9count').textContent = Math.max(0, parseInt(sec, 10) || 0);
});
api.onReminderHide(() => closeRemind());
$('#r9keep').addEventListener('click', async () => {
  closeRemind();
  try { await api.keepWorking(); } catch (_) {}
  toast('已重置提醒计时');
});
api.onAutoStopped(async pending => {
  closeRemind();
  setClipIdle();
  state.pendingStage = pending;
  await presentF3(pending);
});
api.onTrayToggle(() => toggleTimer());

/* =====================================================================
   P6 汇总看板
   ===================================================================== */
const AXIS_PC = 6;
function pctOf(v, s) {
  return ((v - s.axis.min) / s.axis.span) * 100;
}
function renderGantt(s) {
  const box = $('#gantt');
  if (!s.rows.length) {
    box.innerHTML = '<div class="chart-empty">本周期暂无工时记录</div>';
    return;
  }
  const n = s.axis.ticks.length;
  let html = '<div class="gantt"><div class="gantt__head"><div class="gantt__lab"></div><div class="gantt__axis">';
  s.axis.ticks.forEach((t, i) => {
    const isLast = i === n - 1;
    const pos = isLast ? 100 : pctOf(t, s);
    const shift = isLast ? '-100%' : (i === 0 ? '0' : '-50%');
    html += `<span class="gantt__tick" style="left:${Math.max(0, Math.min(100, pos))}%;transform:translateX(${shift})">${esc(s.axis.labels[i])}</span>`;
  });
  html += '</div></div>';
  const shades = ['', 'sh-w1', 'sh-d1', 'sh-w2'];
  s.rows.forEach((row, ri) => {
    html += `<div class="gantt__row"><div class="gantt__rowlab" title="${esc(row.projectName)}">${esc(row.projectName)}</div><div class="gantt__track">`;
    s.axis.grid.forEach(g => {
      if (g < s.axis.min - 0.001 || g > s.axis.min + s.axis.span + 0.001) return;
      html += `<span class="gantt__grid" style="left:${pctOf(g, s)}%"></span>`;
    });
    const dayBars = s.axis.unit === '日' && Array.isArray(row.dayTasks) && row.dayTasks.length;
    if (dayBars) {
      /* 周 / 月：一个项目一天只画一条，不标任务名；当日任务清单放到悬浮浮窗 */
      row.dayTasks.forEach((day, di) => {
        const left = Math.max(0, Math.min(100, pctOf(day.start, s)));
        const width = Math.max(0.7, Math.min(100 - left, pctOf(day.end, s) - left));
        const cls = `pc${((row.colorIndex - 1) % AXIS_PC) + 1}`;
        html += `<div class="gantt__bar ${cls}" style="left:${left}%;width:${width}%" data-r="${ri}" data-d="${di}"></div>`;
      });
    } else {
      /* 当日：仍按任务时间段画细条，但不显示任务名 */
      row.tasks.forEach((task, ti) => {
        const left = Math.max(0, Math.min(100, pctOf(task.start, s)));
        const width = Math.max(0.7, Math.min(100 - left, pctOf(task.end, s) - left));
        const cls = `pc${((row.colorIndex - 1) % AXIS_PC) + 1} ${shades[ti % shades.length]}`;
        html += `<div class="gantt__bar ${cls}" style="left:${left}%;width:${width}%" data-r="${ri}" data-t="${ti}"></div>`;
      });
    }
    html += '</div></div>';
  });
  html += '</div>';
  box.innerHTML = html;
  /* 标签避让 */
  const tickEls = [...box.querySelectorAll('.gantt__tick')];
  const axisW = box.querySelector('.gantt__axis').clientWidth || 1;
  for (let i = 0; i < tickEls.length - 1; i++) {
    const a = tickEls[i], b = tickEls[i + 1];
    const ra = a.getBoundingClientRect(), rb = b.getBoundingClientRect();
    if (ra.right > rb.left - 2) {
      const dx = ra.right - rb.left + 4;
      const cur = parseFloat(a.style.left) || 0;
      a.style.left = (cur - dx / axisW * 100) + '%';
    }
  }
  /* 悬浮浮窗：周 / 月列出当天该项目各任务 + 时长；不显示起止时间 */
  box.querySelectorAll('.gantt__bar').forEach(bar => {
    const row = s.rows[+bar.dataset.r];
    if (!row) return;
    const day = bar.dataset.d != null ? row.dayTasks?.[+bar.dataset.d] : null;
    const task = bar.dataset.t != null ? row.tasks[+bar.dataset.t] : null;
    bar.addEventListener('mouseenter', ev => {
      let htmlTip;
      if (day) {
        const rows = day.tasks.map(t =>
          `<div class="hovercard__row"><span class="hovercard__n">${esc(t.name)}</span><span class="hovercard__v">${esc(t.durationText)}</span></div>`
        ).join('');
        htmlTip = `<div class="hovercard__h">${esc(row.projectName)} · ${esc(day.date)}</div>${rows}
          <div class="hovercard__row hovercard__total"><span class="hovercard__k">合计</span><span class="hovercard__v">${esc(day.durationText)}</span></div>`;
      } else if (task) {
        htmlTip = `<div class="hovercard__h">${esc(row.projectName)}</div>
          <div class="hovercard__row"><span class="hovercard__n">${esc(task.name)}</span><span class="hovercard__v">${esc(task.durationText)}</span></div>`;
      } else {
        return;
      }
      showHoverCard(htmlTip, ev);
    });
    bar.addEventListener('mousemove', moveHoverCard);
    bar.addEventListener('mouseleave', hideHoverCard);
  });
}
function pieColors() {
  const cs = getComputedStyle(document.documentElement);
  const out = [];
  for (let i = 1; i <= 6; i++) {
    const solid = cs.getPropertyValue('--pc' + i).trim();
    const fill = cs.getPropertyValue('--pc' + i + '-f').trim();
    if (solid && fill) out.push({ solid, fill });
  }
  return out;
}
function arcPath(cx, cy, r, a1, a2) {
  const rad = Math.PI / 180;
  const x1 = cx + r * Math.cos(a1 * rad), y1 = cy + r * Math.sin(a1 * rad);
  const x2 = cx + r * Math.cos(a2 * rad), y2 = cy + r * Math.sin(a2 * rad);
  const large = (a2 - a1) > 180 ? 1 : 0;
  return `M ${cx} ${cy} L ${x1.toFixed(2)} ${y1.toFixed(2)} A ${r} ${r} 0 ${large} 1 ${x2.toFixed(2)} ${y2.toFixed(2)} Z`;
}
function fmtPct(v) {
  const s = (v || 0).toFixed(1);
  return (s.endsWith('.0') ? s.slice(0, -2) : s) + '%';
}
function renderPie(s) {
  const svg = $('#pieSvg');
  const leg = $('#pieLeg');
  svg.innerHTML = '';
  leg.innerHTML = '';
  hideHoverCard();
  if (!s.pie.length) {
    leg.innerHTML = '<div class="chart-empty chart-empty--side">本周期暂无数据</div>';
    $('#pieInfo').textContent = '本周期暂无工时记录';
    return;
  }
  const NS = 'http://www.w3.org/2000/svg';
  const cx = 66, cy = 66, r = 55;
  let a = -90;
  const colors = pieColors();
  const total = s.totalDuration || 1;
  s.pie.forEach((g, gi) => {
    const pc = colors[(g.colorIndex - 1) % colors.length] || colors[0];
    const ang = g.duration / total * 360;
    const el = document.createElementNS(NS, ang >= 359.999 ? 'circle' : 'path');
    if (el.tagName === 'circle') {
      el.setAttribute('cx', cx); el.setAttribute('cy', cy); el.setAttribute('r', r);
    } else {
      el.setAttribute('d', arcPath(cx, cy, r, a, a + ang));
    }
    el.setAttribute('fill', pc.fill);
    el.setAttribute('stroke', pc.solid);
    el.setAttribute('class', 'pie-sec');
    el.addEventListener('mouseenter', ev => showHoverCard(pieTipHtml(g), ev));
    el.addEventListener('mousemove', moveHoverCard);
    el.addEventListener('mouseleave', hideHoverCard);
    svg.appendChild(el);
    a += ang;
    leg.insertAdjacentHTML('beforeend',
      `<div class="leg-row"><span class="leg-swat" style="background:${pc.fill};border-color:${pc.solid}"></span>
       <span class="leg-name" title="${esc(g.projectName)}">${esc(g.projectName)}</span>
       <span class="leg-pct">${fmtPct(g.pct)}</span></div>`);
  });
  $('#pieInfo').textContent = `共 ${s.projectCount} 个项目 · ${s.taskCount} 个任务 · ${s.totalDurationText}`;
}
function pieTipHtml(g) {
  return `<div class="hovercard__h">${esc(g.projectName)}</div>` + g.tasks.map(t =>
    `<div class="hovercard__row"><span class="hovercard__n">${esc(t.name)}</span><span class="hovercard__v">${fmtPct(t.pct)}</span></div>`
  ).join('');
}
function showHoverCard(html, e) {
  const tip = $('#hoverTip');
  tip.innerHTML = html;
  tip.classList.add('open');
  moveHoverCard(e);
}
function localPos(el, e) {
  const r = el.getBoundingClientRect();
  const f = (r.width && el.offsetWidth) ? (r.width / el.offsetWidth) : 1;
  return { x: (e.clientX - r.left) / f, y: (e.clientY - r.top) / f };
}
function moveHoverCard(e) {
  const tip = $('#hoverTip');
  if (!tip.classList.contains('open')) return;
  const card = tip.parentElement;
  const p = localPos(card, e);
  const W = card.offsetWidth, H = card.offsetHeight;
  const tw = tip.offsetWidth, th = tip.offsetHeight;
  let x = p.x + 14, y = p.y + 14;
  if (x + tw > W - 6) x = Math.max(6, p.x - tw - 14);
  y = Math.min(y, Math.max(6, H - th - 6));
  tip.style.left = x + 'px';
  tip.style.top = y + 'px';
}
function hideHoverCard() { $('#hoverTip').classList.remove('open'); }

function renderSummary(s) {
  if (!s) return;
  renderGantt(s);
  renderPie(s);
}
let summarySeq = 0;
async function loadSummary() {
  const seq = ++summarySeq;
  try {
    const s = await api.getSummary(state.period);
    if (seq !== summarySeq) return;
    state.lastSummary = s;
    state.summaryStale = false;
    renderSummary(s);
    await loadReport(seq);
  } catch (err) {
    if (seq !== summarySeq) return;
    toast(String(err));
    $('#gantt').innerHTML = '<div class="chart-empty">统计加载失败</div>';
  }
}
$('#periodSeg').addEventListener('click', e => {
  const b = e.target.closest('button'); if (!b) return;
  if (b.classList.contains('active')) return;
  $$('#periodSeg button').forEach(x => x.classList.toggle('active', x === b));
  state.period = b.dataset.p;
  state.summaryStale = true;
  loadSummary();
  toast('已切换至「' + b.dataset.p + '」');
});

/* =====================================================================
   P6 报告：模板初稿 + AI 增强
   ===================================================================== */
function renderReportText() {
  const box = $('#reportText');
  box.textContent = state.reportText || '';
}
async function loadReport(seq) {
  const note = $('#reportNote');
  note.textContent = '正在按模板生成报告…';
  try {
    const r = await api.generateReport(state.period);
    if (seq != null && seq !== summarySeq) return;
    state.reportText = r.text || '';
    renderReportText();
    const count = state.lastSummary ? state.lastSummary.recordCount : 0;
    if (r.usedFallback) {
      note.textContent = '模板读取失败，已回退内置默认模板（报告仍可正常生成）。';
    } else if (count === 0) {
      note.textContent = '本周期暂无计时记录，报告仅含模板标题。';
    } else {
      note.textContent = '默认按固定模板生成；配置 API 后可点「AI 增强」重写，点右上角图标复制文本。';
    }
  } catch (err) {
    if (seq != null && seq !== summarySeq) return;
    state.reportText = '';
    renderReportText();
    note.textContent = '报告生成失败：' + String(err);
  }
}
$('#aiBtn').addEventListener('click', async () => {
  if (state.aiBusy) return;
  if (!state.config?.llm?.apiKey) {
    toast('未配置 API Key，已回退固定模板');
    return;
  }
  if (!state.reportText) { toast('报告尚未生成'); return; }
  state.aiBusy = true;
  const btn = $('#aiBtn');
  const old = btn.textContent;
  const periodAtCall = state.period;
  btn.disabled = true;
  btn.textContent = '重写中…';
  try {
    const text = await api.aiRewrite(periodAtCall, state.reportText);
    if (state.period !== periodAtCall) return;
    state.reportText = text;
    renderReportText();
    $('#reportNote').textContent = '已按 Skill 文件改写（原始事实与数字保持不变）。';
    toast('AI 已增强重写');
  } catch (err) {
    toast(String(err));
  } finally {
    state.aiBusy = false;
    btn.disabled = false;
    btn.textContent = old;
  }
});
$('#copyBtn').addEventListener('click', () => {
  const text = state.reportText || '';
  if (!text.trim()) { toast('报告为空'); return; }
  const done = () => toast('报告已复制到剪贴板');
  if (navigator.clipboard && navigator.clipboard.writeText) {
    navigator.clipboard.writeText(text).then(done).catch(() => { fallbackCopy(text); done(); });
  } else { fallbackCopy(text); done(); }
});
function fallbackCopy(text) {
  const ta = document.createElement('textarea');
  ta.value = text;
  ta.style.position = 'fixed'; ta.style.opacity = '0';
  document.body.appendChild(ta); ta.select();
  try { document.execCommand('copy'); } catch (_) {}
  document.body.removeChild(ta);
}

/* ==============================================================
   初始化
   ============================================================== */
async function reloadRows() {
  const rows = await api.listTasks();
  DATA = rows.map(r => ({ ...r, stages: r.stages || [] }));
  if (state.selectedId && !rowById(state.selectedId)) state.selectedId = null;
  renderTable();
}
function normalizeConfig(c) {
  const d = {
    customFields: [],
    llm: { baseUrl: 'https://api.openai.com/v1', apiKey: '', model: 'gpt-4o-mini' },
    appearance: { theme: 'dark', fontSize: 16 },
    reminder: { workStart: '09:00', workEnd: '18:00', limitXMinutes: 60, stopYSeconds: 60 },
    report: { templateFile: 'config/report-template.yaml', skillFile: 'config/report-skill.md' },
    window: { x: 0, y: 0, saved: false },
  };
  return {
    ...d, ...c,
    llm: { ...d.llm, ...(c?.llm || {}) },
    appearance: { ...d.appearance, ...(c?.appearance || {}) },
    reminder: { ...d.reminder, ...(c?.reminder || {}) },
    report: { ...d.report, ...(c?.report || {}) },
    window: { ...d.window, ...(c?.window || {}) },
    customFields: Array.isArray(c?.customFields) ? c.customFields : [],
  };
}
async function loadConfig() {
  try { state.config = normalizeConfig(await api.getConfig()); }
  catch (_) { state.config = normalizeConfig(null); }
  state.customFields = state.config.customFields.slice();
  try { state.appInfo = await api.appInfo(); } catch (_) { state.appInfo = null; }
  applyTheme(state.config.appearance.theme || 'dark', { silent: true });
  const fs = Math.min(20, Math.max(14, parseInt(state.config.appearance.fontSize, 10) || 16));
  applyFontSize(fs);
  $('#fsRange').value = fs;
}
async function boot() {
  fillTimeSelect($('#setStart'), '09:00');
  fillTimeSelect($('#setEnd'), '18:00');
  await loadConfig();
  resetNewForm();
  renderFieldList();
  await reloadRows();
  /* 计时 / 待保存状态恢复 */
  try {
    const t = await api.timerState();
    if (t && t.running) {
      const id = t.projectId + '-' + t.taskId;
      const row = rowById(id);
      state.selectedId = id;
      state.seconds = t.elapsed || 0;
      if (row) setClipTiming(row);
      else {
        state.timerRunning = true;
        state.timerTaskId = id;
        emitClipState();
      }
    } else if (t && t.pending) {
      await presentF3(t.pending);
    }
    if (t && t.remindActive) await openRemind(t.remindRemaining || 60);
  } catch (_) {}
  api.onEvent('toggle-board', () => toggleBoard());
  api.onEvent('clip-toggle-timer', () => toggleTimer());
  api.onEvent('clip-ready', () => emitClipState());
  api.onEvent('board-force-closed', () => {
    state.boardOpen = false;
    state.detailDirty = false;
    state.editingId = null;
    state.detailSnapshot = null;
    state.showNewRow = false;
    state.clampEnabled = true;
    closeAllSheets();
    unit.classList.remove('is-open');
  });
  emitClipState();               // 让悬浮球窗口拿到当前状态
  if (!api.isDesktop) await openBoardNow();   // 浏览器预览直接看板子
}
boot();
