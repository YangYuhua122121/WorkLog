/* ==============================================================
   workLog · 悬浮球窗口
   --------------------------------------------------------------
   只负责悬浮球本身的显示、拖动、点击；计时与业务状态在主面板窗口。
   两个窗口通过 Tauri event 通信。
   ============================================================== */
import { api } from './api.js';

const $ = s => document.querySelector(s);
const clip = $('#clip');

const state = {
  themePref: 'dark',
  running: false,
  seconds: 0,
  taskId: null,
  proj: '',
  task: '',
  winW: 138,
  winH: 66,
  scale: 1,
  idleW: 138,
  transitionUntil: 0,
};

function pad(n) { return String(n).padStart(2, '0'); }
function fmtHM(sec) {
  const h = Math.floor(sec / 3600), m = Math.floor(sec % 3600 / 60);
  return pad(h) + ':' + pad(m);
}
let toastTimer = null;
function toast(msg) {
  const t = $('#toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => t.classList.remove('show'), 2000);
}
const mqLight = window.matchMedia('(prefers-color-scheme: light)');
function applyAppearance(a) {
  if (!a) return;
  state.themePref = a.theme || 'dark';
  const resolved = state.themePref === 'auto' ? (mqLight.matches ? 'light' : 'dark') : state.themePref;
  document.documentElement.setAttribute('data-theme', resolved);
  const fs = Math.min(20, Math.max(14, parseInt(a.fontSize, 10) || 16));
  state.fontSize = fs;
  document.documentElement.style.setProperty('--ui-fs', fs + 'px');
  clearTimeout(appearanceResizeTimer);
  appearanceResizeTimer = setTimeout(() => resizeToContent(), 340);
}
if (mqLight.addEventListener) {
  mqLight.addEventListener('change', () => {
    if (state.themePref === 'auto') applyAppearance({ theme: 'auto', fontSize: state.fontSize || 16 });
  });
}

function renderTimer() {
  $('#clipTimer').textContent = fmtHM(state.seconds);
}
function render() {
  clip.classList.toggle('is-timing', state.running);
  $('#clipState').textContent = state.running ? '工作中' : '空闲';
  $('#clipTask').textContent = state.running ? (state.proj + ' · ' + state.task) : '';
  renderTimer();
}
let idleResizeTimer = null;
let stateSeq = 0;
async function applyState(s) {
  if (!s) return;
  const seq = ++stateSeq;
  state.transitionUntil = Date.now() + 340;
  const next = !!s.running;
  const proj = s.proj || '';
  const task = s.task || '';

  if (next && !state.running) {
    // 先把窗口撑到「计时态」尺寸，再让卡片在窗口内平滑展开，避免窗口 resize 撞上内容变化
    clearTimeout(idleResizeTimer);
    const target = measureTiming(proj, task);
    await growTo(target);
    if (seq !== stateSeq) return;
    state.running = true;
    state.seconds = Number(s.elapsed) || 0;
    state.taskId = s.taskId || null;
    state.proj = proj;
    state.task = task;
    render();
    return;
  }

  if (!next && state.running) {
    // 先让卡片缩回空闲态，再延迟把窗口缩小，过程中不会裁切内容
    state.running = false;
    state.seconds = 0;
    state.taskId = null;
    state.proj = proj;
    state.task = task;
    render();
    clearTimeout(idleResizeTimer);
    idleResizeTimer = setTimeout(async () => {
      if (state.running) return;
      const c = measure();
      await resizeWindowTo(c);
      state.idleW = c.w;
    }, 280);
    return;
  }

  state.running = next;
  state.taskId = s.taskId || state.taskId;
  state.proj = proj;
  state.task = task;
  if (next) state.seconds = Number(s.elapsed) || state.seconds;
  render();
}

/* ---------- 窗口尺寸跟随悬浮球内容 ---------- */
let appearanceResizeTimer = null;
function targetOf(node) {
  const r = node.getBoundingClientRect();
  return {
    w: Math.max(100, Math.ceil(r.width) + 20),
    h: Math.max(56, Math.ceil(r.height) + 20),
  };
}
function measure() {
  return targetOf(clip);
}
function measureTiming(proj, task) {
  const clone = clip.cloneNode(true);
  clone.removeAttribute('id');
  clone.classList.add('is-timing');
  Object.assign(clone.style, {
    position: 'fixed', left: '-9999px', top: '0', margin: '0',
    visibility: 'hidden', pointerEvents: 'none',
    transition: 'none', width: 'fit-content',
  });
  const row2 = clone.querySelector('.clip__row2');
  if (row2) row2.style.transition = 'none';
  clone.querySelector('.clip__state').textContent = '工作中';
  clone.querySelector('.clip__task').textContent = proj + ' · ' + task;
  clone.querySelector('.clip__timer').textContent = '00:00';
  document.body.appendChild(clone);
  const c = targetOf(clone);
  clone.remove();
  return c;
}
async function waitWindowSize(w, h) {
  for (let i = 0; i < 12; i++) {
    await new Promise(r => requestAnimationFrame(r));
    if (window.innerWidth >= w - 2 && window.innerHeight >= h - 2) return;
  }
}
async function resizeWindowTo(c) {
  if (Math.abs(c.w - state.winW) < 2 && Math.abs(c.h - state.winH) < 2) return;
  state.winW = c.w;
  state.winH = c.h;
  await api.resizeClipWindow(c.w, c.h);
  await waitWindowSize(c.w, c.h);
}
async function growTo(c) {
  if (c.w <= state.winW && c.h <= state.winH) return;
  await resizeWindowTo(c);
}
async function resizeToContent() {
  if (Date.now() < state.transitionUntil) return;
  const c = state.running ? measureTiming(state.proj, state.task) : measure();
  if (!state.running) state.idleW = c.w;
  await resizeWindowTo(c);
}

/* ---------- 拖动 / 点击 ---------- */
let pressStart = null, dragging = false;
let suppressClickUntil = 0;
let lastToggleAt = 0;
clip.addEventListener('pointerdown', e => {
  if (e.target.closest('.rec-btn')) return;
  if (e.button !== undefined && e.button !== 0) return;
  pressStart = { x: e.clientX, y: e.clientY };
  dragging = false;
});
clip.addEventListener('pointermove', e => {
  if (!pressStart) return;
  const dx = e.clientX - pressStart.x, dy = e.clientY - pressStart.y;
  if (!dragging && (Math.abs(dx) > 5 || Math.abs(dy) > 5)) {
    dragging = true;
    suppressClickUntil = Date.now() + 500;
    pressStart = null;
    api.startWindowDrag();
  }
});
function endPress() {
  if (!pressStart) { dragging = false; return; }
  const wasDrag = dragging;
  dragging = false; pressStart = null;
  if (wasDrag || Date.now() < suppressClickUntil) return;
  const now = Date.now();
  if (now - lastToggleAt < 300) return;
  lastToggleAt = now;
  api.emit('toggle-board');
}
clip.addEventListener('pointerup', endPress);
clip.addEventListener('pointercancel', () => { dragging = false; pressStart = null; });
$('#recBtn').addEventListener('pointerdown', e => e.stopPropagation());
$('#recBtn').addEventListener('click', e => {
  e.stopPropagation();
  api.emit('clip-toggle-timer');
});

/* ---------- 右键菜单 / 位置保存 ---------- */
clip.addEventListener('contextmenu', e => {
  e.preventDefault();
  api.showClipMenu();
});

// Rust 会在松手 450ms 后只发一次「位置已稳定」，避免拖动期间每条 Moved 都走 WebView IPC
api.onEvent('clip-moved-settled', async () => {
  const p = await api.windowOuterPosition();
  if (!p) return;
  // 配置里按「空闲宽度」保存，重启后悬浮球中心不漂
  const idleW = state.idleW;
  const x = state.running
    ? Math.round(p.x + (state.winW - idleW) * state.scale / 2)
    : p.x;
  api.saveWindowPosition(x, p.y);
});

/* ---------- 启动 ---------- */
async function boot() {
  render();
  const mon = await api.monitor();
  state.scale = mon?.scaleFactor || 1;
  let cfg = null;
  try { cfg = await api.getConfig(); } catch (_) {}
  if (cfg?.appearance) applyAppearance(cfg.appearance);
  if (cfg?.window?.saved) {
    let x = cfg.window.x, y = cfg.window.y;
    if (mon) {
      const area = mon.workArea || mon;
      const c = measure();
      const maxX = area.position.x + area.size.width - Math.round(c.w * state.scale);
      const maxY = area.position.y + area.size.height - Math.round(c.h * state.scale);
      x = Math.min(Math.max(x, area.position.x), Math.max(area.position.x, maxX));
      y = Math.min(Math.max(y, area.position.y), Math.max(area.position.y, maxY));
    }
    await api.setWindowPosition(x, y);
  } else if (mon) {
    const c = measure();
    const x = mon.position.x + Math.round((mon.size.width - c.w * state.scale) / 2);
    const y = mon.position.y + Math.round(80 * state.scale);
    await api.setWindowPosition(x, y);
  }
  await resizeToContent();
  state.idleW = measure().w;
  state.winW = state.idleW;

  /* 初始计时状态（不依赖主面板先启动） */
  try {
    const t = await api.timerState();
    if (t && t.running) {
      const rows = await api.listTasks();
      const id = t.projectId + '-' + t.taskId;
      const row = rows.find(r => r.id === id);
      applyState({
        running: true, elapsed: t.elapsed,
        taskId: id, proj: row?.proj || '', task: row?.task || '',
      });
    }
  } catch (_) {}

  api.onTick(sec => {
    if (!state.running) return;
    state.seconds = sec;
    renderTimer();
  });
  api.onEvent('clip-state', applyState);
  api.onEvent('clip-toast', toast);
  api.onEvent('clip-appearance', applyAppearance);
  api.onEvent('timer-auto-stopped', () => applyState({ running: false }));
  api.emit('clip-ready');
  setTimeout(() => { api.emit('clip-ready'); }, 400);
}
boot();
