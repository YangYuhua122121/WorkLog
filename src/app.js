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
  themePref:'dark',
  proj:'全部', pri:'全部', status:'未完成',
  sortKey:'', sortDir:0,
  selectedId:null,
  timerRunning:false, seconds:0, timerTaskId:null,
  clampEnabled:true, savedScrollTop:0,
  showNewRow:false, modalLocked:false,
  detailDirty:false, editingId:null, editingStage:null, loadingForm:false,
  detailSnapshot:null, customFields:[],
  pendingStage:null
};

/* =====================================================================
   通用工具
   ===================================================================== */
let toastTimer = null;
function toast(msg){
  const t = $('#toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(()=>t.classList.remove('show'), 2000);
}
function projectNames(){
  return [...new Set(DATA.map(d=>d.proj))].sort((a,b)=>a.localeCompare(b,'zh'));
}
function ddlDays(ddl){
  if(!ddl) return null;
  const d = new Date(ddl+'T00:00:00');
  const today = new Date(); today.setHours(0,0,0,0);
  return Math.round((d-today)/86400000);
}
function ddlText(ddl){
  const n = ddlDays(ddl);
  if(n===null) return '—';
  if(n<0) return '逾期 '+Math.abs(n)+' 天';
  if(n===0) return '今天';
  return n+' 天';
}
function fmtHMS(sec){
  const h=Math.floor(sec/3600), m=Math.floor(sec%3600/60), s=sec%60;
  return String(h).padStart(2,'0')+':'+String(m).padStart(2,'0')+':'+String(s).padStart(2,'0');
}
function fmtHM(sec){
  const h=Math.floor(sec/3600), m=Math.floor(sec%3600/60);
  return String(h).padStart(2,'0')+':'+String(m).padStart(2,'0');
}
function fmtDur(sec){
  if(sec<=0) return '0 秒';
  const h=Math.floor(sec/3600), m=Math.floor(sec%3600/60), s=sec%60;
  if(h>0) return h+' 小时'+(m?(' '+m+' 分钟'):'');
  if(m>0) return m+' 分钟'+(s?(' '+s+' 秒'):'');
  return s+' 秒';
}
function fmtDurShort(sec){
  const h=Math.floor(sec/3600), m=Math.round(sec%3600/60);
  if(h>0) return h+'h'+(m?m+'m':'');
  return m+'m';
}
function todayStr(){
  const d=new Date();
  return d.getFullYear()+'-'+String(d.getMonth()+1).padStart(2,'0')+'-'+String(d.getDate()).padStart(2,'0');
}

/* =====================================================================
   P2 悬浮球 / 计时器
   ===================================================================== */
/* 计时由 Rust 侧负责（窗口隐藏 / 后台也准），前端只负责显示 */
function toggleTimer(){
  if(guardModal()) return;
  if(state.detailDirty){ toast('有未保存的修改，请先保存或取消'); return; }
  closeAllSheets();
  if(!state.timerRunning) startTimer(); else stopTimer();
}
async function startTimer(){
  if(!state.selectedId){ toast('请先选择项目-任务'); return; }
  const d = DATA.find(x=>x.id===state.selectedId);
  if(!d){ toast('请先选择项目-任务'); return; }
  try{ await api.startTimer(d.projectId, d.taskId); }
  catch(err){ toast(String(err)); return; }
  state.timerRunning = true;
  state.seconds = 0;
  state.timerTaskId = d.id;
  $('#clip').classList.add('is-timing');
  $('#clipState').textContent = '工作中';
  $('#clipTask').textContent = d.proj+' · '+d.task;
  $('#clipTimer').textContent = '00:00';
}
function stopTimer(){
  state.timerRunning = false;
  $('#clip').classList.remove('is-timing');
  $('#clipState').textContent = '空闲';
  $('#clipTimer').textContent = '00:00';
  const id = state.timerTaskId;
  state.timerTaskId = null;
  if(id){
    api.stopTimer()
      .then(p=>presentF3(id, p))
      .catch(e=>toast(String(e)));
  }
}
function renderClipTimer(){
  $('#clipTimer').textContent = fmtHM(state.seconds);
}
/* 每秒心跳 */
api.onTick(sec=>{ state.seconds = sec; renderClipTimer(); });

/* 点击启停按钮 */
$('#recBtn').addEventListener('pointerdown', e=>e.stopPropagation());
$('#recBtn').addEventListener('click', e=>{ e.stopPropagation(); toggleTimer(); });

/* =====================================================================
   P1 展开 / 收起 + 拖动（夹子 + 板子整体）
   ===================================================================== */
const unit = $('#unit');
let pressStart=null, dragging=false, unitBase=null;

function setBoardOpen(open){
  if(!open){
    if(state.modalLocked){ toast('请先保存本次阶段记录'); return; }
    closeAllSheets();
    state.showNewRow = false;
    renderTable();
  }
  unit.classList.toggle('is-open', open);
  api.setWindowSize(480, open ? 760 : 96);
}
function toggleBoard(){
  if(state.modalLocked){ toast('请先保存本次阶段记录'); return; }
  if(state.detailDirty){ toast('有未保存的修改，请先保存或取消'); return; }
  setBoardOpen(!unit.classList.contains('is-open'));
}
$('#clip').addEventListener('pointerdown', e=>{
  if(e.target.closest('.rec-btn')) return;
  if(e.button!==undefined && e.button!==0) return;
  pressStart = {x:e.clientX, y:e.clientY};
  dragging = false;
  try{ $('#clip').setPointerCapture(e.pointerId); }catch(_){}
});
$('#clip').addEventListener('pointermove', e=>{
  if(!pressStart) return;
  const dx = e.clientX-pressStart.x, dy = e.clientY-pressStart.y;
  if(!dragging && (Math.abs(dx)>8 || Math.abs(dy)>8)){
    dragging = true;
    const r = unit.getBoundingClientRect();
    unitBase = {left:r.left, top:r.top};
    unit.classList.add('is-dragging','is-moved');
    unit.style.left = r.left+'px';
    unit.style.top  = r.top+'px';
  }
  if(dragging){
    const pad = 8;
    let nx = unitBase.left+dx, ny = unitBase.top+dy;
    nx = Math.max(pad, Math.min(window.innerWidth-unit.offsetWidth-pad, nx));
    ny = Math.max(pad, Math.min(window.innerHeight-40-pad, ny));
    unit.style.left = nx+'px';
    unit.style.top  = ny+'px';
  }
});
function endPress(){
  if(!pressStart) return;
  const wasDrag = dragging;
  unit.classList.remove('is-dragging');
  dragging = false; pressStart = null;
  if(!wasDrag) toggleBoard();
}
$('#clip').addEventListener('pointerup', endPress);
/* 双击悬浮球：把浮动的球 + 主面板复位到默认位置 */
$('#clip').addEventListener('dblclick', e=>{
  e.stopPropagation();
  unit.classList.remove('is-moved','is-dragging');
  unit.style.left=''; unit.style.top='';
  toast('已回到默认位置');
});
$('#clip').addEventListener('pointercancel', ()=>{
  unit.classList.remove('is-dragging');
  dragging=false; pressStart=null;
});

/* =====================================================================
   P5-1 数据表：筛选 / 排序 / 渲染
   ===================================================================== */
const scrollEl = $('#tableScroll');
/* P4：面板上滑展开期间，主面板数据表冻结——滚轮 / 触摸都不再滚动，避免下方行移到面板底下 */
scrollEl.addEventListener('wheel', e=>{ if(anySheetOpen()) e.preventDefault(); }, {passive:false});
scrollEl.addEventListener('touchmove', e=>{ if(anySheetOpen()) e.preventDefault(); }, {passive:false});
/* 面板开合时同步冻结 / 解冻（含拖动滚动条） */
const sheetObserver = new MutationObserver(()=>{
  scrollEl.classList.toggle('is-locked', anySheetOpen());
  scrollEl.classList.toggle('is-sheet-open', anySheetOpen());
});
$$('.sheet').forEach(s=>sheetObserver.observe(s,{attributes:true,attributeFilter:['class']}));
const FILTER_DEFS = {
  proj:   {state:'proj',   opts:()=>['全部', ...projectNames()]},
  pri:    {state:'pri',    opts:()=>['全部','高','中','低']}
};

function filtered(){
  let list = DATA.slice();
  if(state.proj!=='全部')   list = list.filter(d=>d.proj===state.proj);
  if(state.pri!=='全部')    list = list.filter(d=>d.pri===state.pri);
  if(state.status!=='全部') list = list.filter(d=>d.status===state.status);
  if(state.sortKey==='ddl'){
    list.sort((a,b)=>{
      const av=ddlDays(a.ddl), bv=ddlDays(b.ddl);
      const an = av===null?Number.POSITIVE_INFINITY:av;
      const bn = bv===null?Number.POSITIVE_INFINITY:bv;
      return (an-bn)*state.sortDir;
    });
  }else{
    list.sort((a,b)=>b.lastOp-a.lastOp);
  }
  return list;
}
function renderTable(){
  const list = filtered();
  const top = state.showNewRow
    ? '<tr class="blank blank--new"><td></td><td></td><td></td><td></td></tr>'
    : '';
  const html = list.map(d=>{
    const sel = d.id===state.selectedId ? 'selected' : '';
    const strip = d.status==='已完成' ? '#8e8e93' : '#0a84ff';
    return `<tr data-id="${d.id}" class="${sel}" style="--strip:${strip}">
      <td>${d.proj}</td><td>${d.task}</td><td>${ddlText(d.ddl)}</td>
      <td><span class="badge pri-${d.pri}">${d.pri}</span></td></tr>`;
  }).join('');
  let bot = '';
  for(let i=0;i<6;i++) bot += '<tr class="blank blk"><td></td><td></td><td></td><td></td></tr>';
  $('#tbody').innerHTML = top + html + bot;
  updateHeaderMarks();
}
function updateHeaderMarks(){
  $$('thead th[data-filter]').forEach(th=>{
    const cur = state[FILTER_DEFS[th.dataset.filter].state];
    th.classList.toggle('is-active', cur!=='全部');
  });
  const s = $('thead th[data-sort="ddl"]');
  if(state.sortKey==='ddl'){
    s.classList.add('is-active');
    s.classList.toggle('is-asc', state.sortDir===1);
    s.classList.toggle('is-desc', state.sortDir===-1);
  }else{
    s.classList.remove('is-active');
    s.classList.remove('is-asc');
    s.classList.remove('is-desc');
  }
}

/* 表头筛选下拉 */
const hdrMenu = $('#hdrMenu');
function closeHeaderMenu(){ hdrMenu.classList.remove('open'); }
function openHeaderMenu(th,key){
  const def = FILTER_DEFS[key];
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  const cur = state[def.state];
  hdrMenu.style.top  = headerH+'px';
  hdrMenu.style.left = Math.max(0, th.offsetLeft)+'px';
  hdrMenu.innerHTML = def.opts().map(o=>{
    const on = (o===cur);
    return `<div class="mi ${on?'active':''}" data-v="${o}"><span class="chk">${on?'✓':''}</span>${o}</div>`;
  }).join('');
  hdrMenu.classList.add('open');
  /* 下拉框不超出主面板右边界 */
  const maxLeft = Math.max(0, scrollEl.clientWidth - hdrMenu.offsetWidth - 4);
  hdrMenu.style.left = Math.min(Math.max(0, th.offsetLeft), maxLeft)+'px';
  hdrMenu.querySelectorAll('.mi').forEach(mi=>{
    mi.onclick = ev=>{
      ev.stopPropagation();
      state[def.state] = mi.dataset.v;
      closeHeaderMenu();
      renderTable();
    };
  });
}
$$('thead th[data-filter]').forEach(th=>{
  th.addEventListener('click', e=>{
    e.stopPropagation();
    if(state.modalLocked){ toast('请先保存本次阶段记录'); return; }
    if(hdrMenu.classList.contains('open')){ closeHeaderMenu(); return; }
    openHeaderMenu(th, th.dataset.filter);
  });
});
$('thead th[data-sort="ddl"]').addEventListener('click', e=>{
  e.stopPropagation();
  if(state.modalLocked){ toast('请先保存本次阶段记录'); return; }
  const cur = state.sortKey==='ddl' ? state.sortDir : 0;
  if(cur===0){ state.sortKey='ddl'; state.sortDir=1; }
  else if(cur===1){ state.sortDir=-1; }
  else { state.sortKey=''; state.sortDir=0; }
  renderTable();
});
scrollEl.addEventListener('scroll', closeHeaderMenu);
document.addEventListener('click', e=>{
  if(!e.target.closest('.hdr-menu') && !e.target.closest('th[data-filter]')) closeHeaderMenu();
});

/* 滚动钳制：常规浏览最多滚到「底部两个空行」 */
function maxScrollFor2Blanks(){
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  const visibleBody = scrollEl.clientHeight - headerH;
  const blanks = $('#tbody').querySelectorAll('tr.blank.blk');
  const second = blanks[1];
  if(!second) return scrollEl.scrollHeight;
  return second.offsetTop + second.offsetHeight - headerH - visibleBody;
}
scrollEl.addEventListener('scroll', ()=>{
  if(!state.clampEnabled) return;
  const max = maxScrollFor2Blanks();
  if(scrollEl.scrollTop > max) scrollEl.scrollTop = max;
});

/* =====================================================================
   表格行交互：单击选中 / 再次单击或双击展开详情
   ===================================================================== */
function selectRowVisual(tr){
  $$('#tbody tr.selected').forEach(x=>x.classList.remove('selected'));
  if(tr) tr.classList.add('selected');
}
function guardModal(){
  if(state.modalLocked){ toast('请先保存本次阶段记录'); return true; }
  return false;
}
$('#tbody').addEventListener('click', e=>{
  const tr = e.target.closest('tr');
  if(!tr || tr.classList.contains('blank')) return;
  if(guardModal()) return;
  const id = +tr.dataset.id;
  if(state.timerRunning){
    if(id!==state.timerTaskId){ toast('计时中，无法切换项目'); return; }
    if(state.selectedId===id) openDetail(id);   /* 计时中允许查看本项目详情 */
    return;
  }
  if(state.selectedId===id){ openDetail(id); return; }  /* 再次单击 = 双击 */
  state.selectedId = id;
  selectRowVisual(tr);
});
$('#tbody').addEventListener('dblclick', e=>{
  const tr = e.target.closest('tr');
  if(!tr || tr.classList.contains('blank')) return;
  if(guardModal()) return;
  const id = +tr.dataset.id;
  if(state.timerRunning && id!==state.timerTaskId){ toast('计时中，无法切换项目'); return; }
  openDetail(id);
});

/* =====================================================================
   上滑面板通用
   ===================================================================== */
function closeAllSheets(){
  ['sheet-detail','sheet-new','sheet-f3','sheet-settings'].forEach(id=>$('#'+id).classList.remove('open'));
}
function anySheetOpen(){ return !!document.querySelector('.sheet.open'); }
function rememberScroll(){ state.savedScrollTop = scrollEl.scrollTop; }
function restoreScroll(){
  $('#tableSpacer').style.height = '0px';      /* 关闭面板后撤掉补足的滚动空间，恢复常规浏览范围 */
  scrollEl.scrollTop = state.savedScrollTop || 0;
}
function ensureTaskTab(){
  $$('.tab').forEach(x=>x.classList.toggle('active', x.dataset.tab==='task'));
  $$('.tabpane').forEach(x=>x.classList.toggle('active', x.id==='pane-task'));
  $('#newBtn').classList.remove('hidden');
}
function sheetTopForRow(row){
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  return headerH + (row ? row.offsetHeight : 40);
}
/* 让指定行能滚动到表头正下方：必要时在表格底部补足可滚动空间 */
function ensureScrollRoom(row){
  const headerH = scrollEl.querySelector('thead').offsetHeight;
  const spacer = $('#tableSpacer');
  spacer.style.height = '0px';                 /* 先归零再测量，否则重复打开时会算错 */
  const need = Math.max(0, row.offsetTop - headerH);
  const shortfall = need + scrollEl.clientHeight - scrollEl.scrollHeight;
  spacer.style.height = Math.max(0, shortfall + 2) + 'px';
  return need;
}
function closeAndRestoreAll(){
  const hadNew = state.showNewRow;
  state.showNewRow = false;
  closeAllSheets();
  state.clampEnabled = true;
  state.detailDirty = false;
  state.editingId = null;
  state.detailSnapshot = null;
  if(hadNew) renderTable();
  restoreScroll();
}
function closeCleanSheets(){
  closeAllSheets();
  state.clampEnabled = true;
}

/* =====================================================================
   P5-2 项目详细信息
   ===================================================================== */
/* 项目名：可输入 + 仅点击右侧箭头弹出下拉 */
function buildComboList(listEl, inputEl){
  const names = projectNames();
  listEl.innerHTML = names.length
    ? names.map(n=>`<div class="ci" data-v="${n}">${n}</div>`).join('')
    : '<div class="ci empty">暂无已有项目，可直接输入新建</div>';
  listEl.querySelectorAll('.ci[data-v]').forEach(ci=>{
    ci.onclick = ev=>{
      ev.stopPropagation();
      inputEl.value = ci.dataset.v;
      ci.closest('.combo').classList.remove('open');
      inputEl.dispatchEvent(new Event('input', {bubbles:true}));
    };
  });
}
function closeCombos(){ $$('.combo.open').forEach(c=>c.classList.remove('open')); }
function toggleCombo(btn){
  const combo = btn.closest('.combo');
  const input = combo.querySelector('input');
  const list = combo.querySelector('.combo-list');
  const wasOpen = combo.classList.contains('open');
  closeCombos();
  if(wasOpen) return;
  buildComboList(list, input);
  combo.classList.add('open');
}
$$('.combo-arrow').forEach(b=>{
  b.addEventListener('click', e=>{ e.stopPropagation(); toggleCombo(b); });
});
document.addEventListener('click', e=>{
  if(e.target.closest('.combo-arrow') || e.target.closest('.combo-list')) return;
  closeCombos();
});
/* 自定义字段：每个字段完整占一行，显示在备注下方 */
function customValues(containerId){
  const o = {};
  $$('#'+containerId+' input[data-cf]').forEach(i=>{ o[i.dataset.cf] = i.value; });
  return o;
}
function renderCustomFields(containerId, values){
  const box = $('#'+containerId);
  if(!box) return;
  if(!state.customFields.length){ box.innerHTML=''; return; }
  box.innerHTML = state.customFields.map(name=>{
    const raw = (values && values[name]!=null) ? String(values[name]) : '';
    const v = raw.replace(/&/g,'&amp;').replace(/"/g,'&quot;').replace(/</g,'&lt;');
    return `<div class="fld full" style="margin-top:10px"><label>${name}</label><input type="text" data-cf="${name}" value="${v}" placeholder="选填"></div>`;
  }).join('');
}
function fillTri(sel, value){
  $$(sel+' button').forEach(b=>b.classList.toggle('active', b.dataset.v===value));
}
function activeTri(sel){
  const b = $(sel+' button.active');
  return b ? b.dataset.v : '';
}
function markDetailDirty(){
  if(state.loadingForm) return;
  if(state.detailDirty) return;
  state.detailDirty = true;
  $('#detailCancel').disabled = false;
  $('#detailSave').disabled = false;
  $('#detailHint').textContent = '有未保存的修改';
}
function loadDetailForm(item){
  state.loadingForm = true;
  $('#dProj').value = item.proj || '';
  $('#dTask').value = item.task || '';
  $('#dDDL').value = item.ddl || '';
  $('#dNote').value = item.note || '';
  renderCustomFields('dCustomFields', item.custom);
  fillTri('#dPri', item.pri);
  fillTri('#sdStatus', item.status);
  renderDetailStages(item);
  const idx = item.stages.length ? item.stages.length-1 : null;
  selectStage(idx);
  state.loadingForm = false;
  state.detailDirty = false;
  $('#detailCancel').disabled = true;
  $('#detailSave').disabled = true;
  $('#detailHint').textContent = '仅在有改动时可操作';
}
function renderDetailStages(item){
  const body = $('#dStageBody');
  $('#dStageCount').textContent = `共 ${item.stages.length} 段`;
  if(!item.stages.length){
    body.innerHTML = '<tr><td colspan="3" style="color:#888;font-size:11px;padding:8px 6px">暂无阶段记录（开始计时后自动生成）</td></tr>';
    return;
  }
  body.innerHTML = item.stages.map((s,i)=>
    `<tr class="stage-row" data-idx="${i}"><td>阶段 ${s.no}</td><td>${s.date}</td><td title="${s.out||''}">${s.out||'—'}</td></tr>`
  ).join('');
  body.querySelectorAll('tr.stage-row').forEach(tr=>{
    tr.onclick = ()=>{
      /* 允许直接切换阶段：先把当前阶段输入框的内容写回内存（保存 / 取消前不落库），切换不丢内容 */
      const next = +tr.dataset.idx;
      commitStageDraft();
      state.loadingForm = true;
      selectStage(next);
      state.loadingForm = false;
    };
  });
  body.querySelectorAll('tr.stage-row').forEach(tr=>{
    tr.classList.toggle('sel', +tr.dataset.idx===state.editingStage);
  });
}
/* 切换阶段前：把当前阶段的编辑内容写回内存（保存前不落库），保证切换不丢内容 */
function commitStageDraft(){
  const item = DATA.find(x=>x.id===state.editingId);
  const idx = state.editingStage;
  if(!item || idx===null || idx===undefined || !item.stages[idx]) return;
  if(!state.detailDirty) return;
  item.stages[idx].out  = $('#sdOut').value;
  item.stages[idx].next = $('#sdNext').value;
  renderDetailStages(item);
}
function selectStage(idx){
  const item = DATA.find(x=>x.id===state.editingId);
  if(!item) return;
  state.editingStage = idx;
  $$('#dStageBody tr.stage-row').forEach(tr=>tr.classList.toggle('sel', +tr.dataset.idx===idx));
  const s = (idx===null || idx===undefined) ? null : item.stages[idx];
  if(!s){
    $('#sdNo').textContent = '—';
    $('#sdDur').textContent = '—';
    $('#sdOut').value = '';
    $('#sdNext').value = '';
    $('#sdOut').disabled = true;
    $('#sdNext').disabled = true;
    $$('#sdStatus button').forEach(b=>b.disabled = true);
    return;
  }
  $('#sdNo').textContent = '阶段 '+s.no;
  $('#sdDur').textContent = fmtDur(s.durSec);
  $('#sdOut').value = s.out || '';
  $('#sdNext').value = s.next || '';
  $('#sdOut').disabled = false;
  $('#sdNext').disabled = false;
  $$('#sdStatus button').forEach(b=>b.disabled = false);
}

async function openDetail(id){
  if(guardModal()) return;
  /* 双击会先触发两次 click 再触发 dblclick，这里做幂等，避免重复滚动把行位置算错 */
  if($('#sheet-detail').classList.contains('open') && state.editingId===id) return;
  if($('#sheet-detail').classList.contains('open') && state.detailDirty){
    toast('有未保存的修改，请先保存或取消'); return;
  }
  const srcRow = DATA.find(x=>x.id===id);
  if(!srcRow) return;
  let item = srcRow;
  try{ item = await api.getTask(srcRow.projectId, srcRow.taskId); }
  catch(err){ toast(String(err)); return; }
  const _i = DATA.findIndex(x=>x.id===id);
  if(_i>=0) DATA[_i] = item;
  state.detailSnapshot = JSON.stringify(item);   /* 打开时快照，取消时回滚阶段改动 */
  rememberScroll();
  closeAllSheets();
  unit.classList.add('is-open');
  ensureTaskTab();
  state.clampEnabled = false;
  state.showNewRow = false;
  state.selectedId = id;
  state.editingId = id;
  closeHeaderMenu();
  renderTable();
  const row = $('#tbody').querySelector(`tr[data-id="${id}"]`);
  if(row) scrollEl.scrollTop = ensureScrollRoom(row);
  loadDetailForm(item);
  $('#sheet-detail').style.top = sheetTopForRow(row)+'px';
  $('#sheet-detail').classList.add('open');
}
function closeDetailAndRestore(keepChanges){
  /* 取消：把切换阶段时写进内存的临时改动回滚到打开前的快照；保存：保留 */
  if(!keepChanges && state.detailSnapshot){
    const snapshotItem = DATA.find(x=>x.id===state.editingId);
    if(snapshotItem) Object.assign(snapshotItem, JSON.parse(state.detailSnapshot));
  }
  state.detailSnapshot = null;
  state.detailDirty = false;
  state.editingId = null;
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
}
$('#detailClose').addEventListener('click', ()=>{
  if(state.detailDirty){ toast('有未保存的修改，请先保存或取消'); return; }
  closeDetailAndRestore();
});
$('#detailCancel').addEventListener('click', ()=>closeDetailAndRestore(false));
$('#detailSave').addEventListener('click', async ()=>{
  const item = DATA.find(x=>x.id===state.editingId);
  if(!item) return;
  const payload = {
    projectName: $('#dProj').value.trim() || item.proj,
    taskName: $('#dTask').value.trim() || item.task,
    dueDate: $('#dDDL').value || null,
    priority: activeTri('#dPri') || item.pri,
    status: activeTri('#sdStatus') || item.status,
    remark: $('#dNote').value,
    customFields: JSON.stringify(customValues('dCustomFields')),
  };
  try{ await api.updateTask(item.projectId, item.taskId, payload); }
  catch(err){ toast(String(err)); return; }
  Object.assign(item, {
    proj:payload.projectName, task:payload.taskName, ddl:payload.dueDate||'',
    pri:payload.priority, status:payload.status, note:payload.remark||'',
    custom:customValues('dCustomFields'), lastOp:Date.now()
  });
  closeDetailAndRestore(true);
  toast('已保存修改');
});
/* 详情字段变更 → 启用保存/取消 */
['dProj','dTask','dDDL','dNote','sdOut','sdNext'].forEach(id=>{
  $('#'+id).addEventListener('input', markDetailDirty);
  $('#'+id).addEventListener('change', markDetailDirty);
});
$('#dCustomFields').addEventListener('input', markDetailDirty);
$('#dPri').addEventListener('click', e=>{
  const b = e.target.closest('button'); if(!b) return;
  fillTri('#dPri', b.dataset.v);
  markDetailDirty();
});
$('#sdStatus').addEventListener('click', e=>{
  const b = e.target.closest('button'); if(!b) return;
  fillTri('#sdStatus', b.dataset.v);
  markDetailDirty();
});

/* =====================================================================
   P7 新建项目 - 任务
   ===================================================================== */
function resetNewForm(){
  $('#nProj').value = '';
  $('#nTask').value = '';
  $('#nDDL').value = '';
  $('#nNote').value = '';
  fillTri('#nPri', '中');
  renderCustomFields('nCustomFields', null);
  closeCombos();
}
function openNewSheet(){
  if(guardModal()) return;
  if(state.detailDirty){ toast('有未保存的修改，请先保存或取消'); return; }
  if($('#sheet-new').classList.contains('open')){ closeNewSheet(); return; }
  rememberScroll();
  closeAllSheets();
  unit.classList.add('is-open');
  ensureTaskTab();
  state.clampEnabled = false;
  state.showNewRow = true;
  renderCustomFields('nCustomFields', customValues('nCustomFields'));
  renderTable();
  scrollEl.scrollTop = 0;
  const row = $('#tbody').querySelector('tr.blank--new');
  $('#sheet-new').style.top = sheetTopForRow(row)+'px';
  $('#sheet-new').classList.add('open');
  setTimeout(()=>$('#nTask').focus(), 120);
}
function closeNewSheet(){
  state.showNewRow = false;
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
}
$('#nPri').addEventListener('click', e=>{
  const b = e.target.closest('button'); if(!b) return;
  fillTri('#nPri', b.dataset.v);
});
$('#newBtn').addEventListener('click', e=>{ e.stopPropagation(); openNewSheet(); });
$('#newClose').addEventListener('click', closeNewSheet);
$('#newCancel').addEventListener('click', ()=>{
  resetNewForm();
  state.showNewRow = false;
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
});
$('#newSave').addEventListener('click', async ()=>{
  const proj = $('#nProj').value.trim();
  const task = $('#nTask').value.trim();
  if(!proj){ toast('请填写项目名'); return; }
  if(!task){ toast('请填写任务名'); return; }
  let row;
  try{
    row = await api.createTask({
      projectName: proj, taskName: task,
      dueDate: $('#nDDL').value || null,
      priority: activeTri('#nPri') || '中',
      remark: $('#nNote').value,
      customFields: JSON.stringify(customValues('nCustomFields')),
    });
  }catch(err){ toast(String(err)); return; }
  DATA.unshift(row);
  state.selectedId = row.id;
  state.showNewRow = false;
  resetNewForm();
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  scrollEl.scrollTop = 0;
  toast(`已保存：P${String(row.projectId).padStart(3,'0')}-T${String(row.taskId).padStart(2,'0')}`);
});

/* =====================================================================
   P8 设置
   ===================================================================== */
function openSettings(){
  if(guardModal()) return;
  if(state.detailDirty){ toast('有未保存的修改，请先保存或取消'); return; }
  if($('#sheet-settings').classList.contains('open')){ closeAndRestoreAll(); return; }
  rememberScroll();
  closeAllSheets();
  closeHeaderMenu();
  state.showNewRow = false;
  unit.classList.add('is-open');
  state.clampEnabled = false;
  renderTable();
  $('#sheet-settings').classList.add('open');
}
$('#setBtn').addEventListener('click', e=>{ e.stopPropagation(); openSettings(); });
$('#setClose').addEventListener('click', closeAndRestoreAll);
$('#setSave').addEventListener('click', async ()=>{
  const cfg = await api.getConfig();
  cfg.llm = {
    baseUrl: $('#setBase').value.trim(),
    apiKey:  $('#setKey').value.trim(),
    model:   $('#setModel').value.trim(),
  };
  cfg.appearance = { theme: state.themePref, fontSize: parseInt($('#fsRange').value,10) || 16 };
  cfg.reminder = {
    workStart: $('#setStart').value,
    workEnd:   $('#setEnd').value,
    limitXMinutes: parseInt($('#setX').value,10) || 60,
    stopYSeconds:  parseInt($('#setY').value,10) || 60,
  };
  try{ await api.saveConfig(cfg); }catch(err){ toast(String(err)); return; }
  closeAndRestoreAll();
  toast('设置已保存');
});
$('#setFieldAdd').addEventListener('click', ()=>{
  const v = $('#setField').value.trim();
  if(!v){ toast('请输入字段名'); return; }
  if(state.customFields.includes(v)){ toast('该字段已存在'); return; }
  state.customFields.push(v);
  $('#setField').value = '';
  renderFieldList();
  syncCustomFieldInputs();
  toast('已添加自定义字段：'+v);
});
function renderFieldList(){
  const box = $('#setFieldList');
  if(!state.customFields.length){ box.innerHTML = '<span class="note">暂无自定义字段</span>'; return; }
  box.innerHTML = state.customFields.map((n,i)=>
    `<span class="field-chip">${n}<button type="button" data-i="${i}" title="删除">×</button></span>`
  ).join('');
  box.querySelectorAll('button[data-i]').forEach(b=>{
    b.onclick = ()=>{
      state.customFields.splice(+b.dataset.i,1);
      renderFieldList();
      syncCustomFieldInputs();
    };
  });
}
/* 字段列表变化后，同步「新建 / 详情」两个面板的字段输入区（保留已填内容） */
function syncCustomFieldInputs(){
  renderCustomFields('nCustomFields', customValues('nCustomFields'));
  if($('#sheet-detail').classList.contains('open')) renderCustomFields('dCustomFields', customValues('dCustomFields'));
}

/* =====================================================================
   外部点击拦截（P4）：只关闭、不触发底层；模态下完全拦截
   ===================================================================== */
function blockOutside(e){
  if(!anySheetOpen()) return;
  if(e.target.closest('.sheet')) return;
  if(e.target.closest('.tool-ic')) return;
  /* 标签栏不豁免：P4 要求「切标签」在关闭面板的这次点击里不生效 */
  if(state.modalLocked){
    if(e.type==='click') toast('请先保存本次阶段记录');
    e.stopPropagation(); e.preventDefault();
    return;
  }
  if($('#sheet-detail').classList.contains('open') && state.detailDirty){
    if(e.type==='click') toast('有未保存的修改，请先保存或取消');
    e.stopPropagation(); e.preventDefault();
    return;
  }
  e.stopPropagation(); e.preventDefault();
  if(e.type==='click') closeAndRestoreAll();
}
$('#board').addEventListener('mousedown', blockOutside, true);
$('#board').addEventListener('click', blockOutside, true);

/* =====================================================================
   标签页切换
   ===================================================================== */
$$('.tab').forEach(t=>t.addEventListener('click', ()=>{
  if(guardModal()) return;
  if(state.detailDirty){ toast('有未保存的修改，请先保存或取消'); return; }
  closeAllSheets();
  state.showNewRow = false;
  state.clampEnabled = true;
  $('#tableSpacer').style.height = '0px';
  $$('.tab').forEach(x=>x.classList.toggle('active', x===t));
  $$('.tabpane').forEach(x=>x.classList.toggle('active', x.id==='pane-'+t.dataset.tab));
  $('#newBtn').classList.toggle('hidden', t.dataset.tab==='summary');
  renderTable();
  /* M1：统计看板尚未接入，汇总页先显示占位说明 */
  const pane = $('#pane-summary');
  if(pane){
    [...pane.children].forEach(c=>{
      c.style.display = (c.id==='summaryPlaceholder')
        ? (t.dataset.tab==='summary' ? 'flex' : 'none')
        : (t.dataset.tab==='summary' ? 'none' : '');
    });
  }
}));

/* =====================================================================
   P5-3 本次阶段记录（停止计时后 · 模态）
   ===================================================================== */
function renderF3Stages(item, pendingNo){
  const body = $('#f3StageBody');
  let html = item.stages.map(s=>
    `<tr><td>阶段 ${s.no}</td><td>${s.date}</td><td>${s.out||'—'}</td></tr>`
  ).join('');
  html += `<tr class="stage-row sel"><td>阶段 ${pendingNo}</td><td>${todayStr()}</td><td>本次 · 保存后生成</td></tr>`;
  body.innerHTML = html;
  $('#f3StageHint').textContent = `已有 ${item.stages.length} 段 + 本次`;
}
function presentF3(taskId, pending){
  const item = DATA.find(x=>x.id===taskId);
  if(!item) return;
  state.modalLocked = true;
  rememberScroll();
  closeRemind9();
  closeAllSheets();
  unit.classList.add('is-open');
  ensureTaskTab();
  state.clampEnabled = false;
  state.showNewRow = false;
  state.selectedId = taskId;
  closeHeaderMenu();
  renderTable();
  const row = $('#tbody').querySelector(`tr[data-id="${taskId}"]`);
  if(row) scrollEl.scrollTop = ensureScrollRoom(row);
  const pendingNo = item.stages.length + 1;
  $('#f3Task').textContent = item.proj+' · '+item.task;
  $('#f3DDL').textContent = ddlText(item.ddl);
  $('#f3Pri').textContent = item.pri;
  $('#f3Note').textContent = item.note || '—';
  renderF3Stages(item, pendingNo);
  $('#f3No').textContent = '阶段 '+pendingNo;
  state.pendingStage = pending || null;
  $('#f3Dur').textContent = fmtDur(pending ? pending.duration : state.seconds);
  const last = item.stages[item.stages.length-1];
  $('#f3Out').value = last ? (last.next||'') : '';
  $('#f3Next').value = '';
  fillTri('#f3Status','未完成');
  $('#sheet-f3').style.top = sheetTopForRow(row)+'px';
  $('#sheet-f3').classList.add('open');
  setTimeout(()=>{ try{ $('#f3Out').focus(); }catch(_){} }, 150);
}
$('#f3Status').addEventListener('click', e=>{
  const b = e.target.closest('button'); if(!b) return;
  fillTri('#f3Status', b.dataset.v);
});
$('#f3Save').addEventListener('click', async ()=>{
  const item = DATA.find(x=>x.id===state.selectedId);
  if(!item){ state.modalLocked=false; return; }
  const no = item.stages.length+1;
  const p = state.pendingStage;
  const nowIso = ()=>new Date().toISOString().slice(0,19).replace('T',' ');
  try{
    await api.addStage({
      projectId: item.projectId, taskId: item.taskId,
      startedAt: p ? p.startedAt : nowIso(),
      endedAt:   p ? p.endedAt   : nowIso(),
      duration:  p ? p.duration  : state.seconds,
      result:    $('#f3Out').value,
      nextGoal:  $('#f3Next').value,
      status:    activeTri('#f3Status') || '未完成',
    });
  }catch(err){ toast(String(err)); return; }
  item.status = activeTri('#f3Status') || item.status;
  item.lastOp = Date.now();
  state.modalLocked = false;
  state.seconds = 0;
  state.pendingStage = null;
  closeAllSheets();
  state.clampEnabled = true;
  await reloadRows();
  restoreScroll();
  toast('已保存阶段 '+no);
});
$('#f3Delete').addEventListener('click', async ()=>{
  state.modalLocked = false;
  state.seconds = 0;
  state.pendingStage = null;
  try{ await api.discardPending(); }catch(_){}
  closeAllSheets();
  state.clampEnabled = true;
  renderTable();
  restoreScroll();
  toast('已删除本次阶段记录（不计入累计）');
});

/* =====================================================================
   P9 计时提醒窗（防遗忘）
   ===================================================================== */
let r9timer = null;
function openRemind9(){
  if(state.modalLocked){ toast('请先保存本次阶段记录'); return; }
  closeRemind9();
  const y = Math.max(5, parseInt($('#setY').value,10) || 60);
  let n = y;
  $('#r9count').textContent = n;
  $('#remind9').classList.add('open');
  r9timer = setInterval(()=>{
    n--;
    $('#r9count').textContent = Math.max(0,n);
    if(n<=0){
      closeRemind9();
      if(state.timerRunning){ stopTimer(); }
      else{ toast('提醒倒计时结束'); }
    }
  }, 1000);
}
function closeRemind9(){
  clearInterval(r9timer); r9timer = null;
  $('#remind9').classList.remove('open');
}
/* 只保留「仍在工作」：人不在 → 倒计时到 0 自动停止并进入 F3；人在且想停 → 点悬浮球启停按钮 */
$('#r9keep').addEventListener('click', ()=>{ closeRemind9(); toast('已重置提醒计时'); });

/* =====================================================================
   外观：深色 / 浅色 / 跟随系统
   ===================================================================== */
const mqLight = window.matchMedia('(prefers-color-scheme: light)');
function resolveTheme(pref){
  return pref==='auto' ? (mqLight.matches ? 'light' : 'dark') : pref;
}
function applyTheme(pref, opts){
  state.themePref = pref || 'dark';
  document.documentElement.setAttribute('data-theme', resolveTheme(state.themePref));
  $$('#themeSeg button').forEach(b=>b.classList.toggle('active', b.dataset.v===state.themePref));
  if(!(opts && opts.silent)){
    /* 看板未接入，无需重绘 */
  }
}
/* 界面字号：滑条改写根字号，界面所有字号用 rem 跟随 */
const fsRange = $('#fsRange');
function applyFontSize(px){
  const v = Math.min(20, Math.max(14, parseInt(px,10) || 16));
  document.documentElement.style.setProperty('--ui-fs', v+'px');
  $('#fsVal').textContent = v+'px';
}
fsRange.addEventListener('input', ()=>applyFontSize(fsRange.value));
applyFontSize(fsRange.value);

/* 报告生成配置：打开模板 / Skill 文件（原型演示仅提示路径） */
function openConfigFile(el, what){
  toast('（原型演示）实际应用将用系统默认程序打开：' + el.value);
}
$('#btnOpenTemplate').addEventListener('click', ()=>openConfigFile($('#setTemplatePath'), '模板'));
$('#btnOpenSkill').addEventListener('click', ()=>openConfigFile($('#setSkillPath'), 'Skill'));

$('#themeSeg').addEventListener('click', e=>{
  const b = e.target.closest('button'); if(!b) return;
  applyTheme(b.dataset.v);
  toast('外观：' + ({dark:'深色', light:'浅色', auto:'跟随系统'})[b.dataset.v]);
});
/* 跟随系统时，系统外观变化实时同步 */
if(mqLight.addEventListener){
  mqLight.addEventListener('change', ()=>{ if(state.themePref==='auto') applyTheme('auto'); });
}

/* ==============================================================
   初始化
   ============================================================== */
/** 重新拉数据表 */
async function reloadRows(){
  const rows = await api.listTasks();
  DATA = rows.map(r=>({...r, stages: r.stages || []}));
  renderTable();
}
/** 读配置 → 外观 / 提醒 / LLM */
async function loadConfig(){
  let cfg = null;
  try{ cfg = await api.getConfig(); }catch(_){}
  if(!cfg) return;
  state.themePref = cfg.appearance?.theme || 'dark';
  const fs = parseInt(cfg.appearance?.fontSize, 10) || 16;
  const range = $('#fsRange');
  if(range) range.value = fs;
  document.documentElement.style.setProperty('--ui-fs', fs+'px');
  const fsVal = $('#fsVal'); if(fsVal) fsVal.textContent = fs+'px';
  const set = (sel, v)=>{ const el = $(sel); if(el && v!==undefined && v!==null) el.value = v; };
  set('#setStart', cfg.reminder?.workStart);
  set('#setEnd', cfg.reminder?.workEnd);
  set('#setX', cfg.reminder?.limitXMinutes);
  set('#setY', cfg.reminder?.stopYSeconds);
  set('#setBase', cfg.llm?.baseUrl);
  set('#setKey', cfg.llm?.apiKey);
  set('#setModel', cfg.llm?.model);
}
/* 上下班时间下拉：上班 06:00–12:00、下班 15:00–23:30（半小时粒度） */
function fillTimeSelect(el, from, to, def){
  if(!el) return;
  let html = '';
  for(let m=from; m<=to; m+=30){
    const v = String(Math.floor(m/60)).padStart(2,'0') + ':' + String(m%60).padStart(2,'0');
    html += '<option value="'+v+'"'+(v===def?' selected':'')+'>'+v+'</option>';
  }
  el.innerHTML = html;
}
async function boot(){
  fillTimeSelect($('#setStart'), 6*60, 12*60, '09:00');
  fillTimeSelect($('#setEnd'), 15*60, 23*60+30, '18:00');
  await loadConfig();
  applyTheme(state.themePref, {silent:true});
  resetNewForm();
  renderFieldList();
  await reloadRows();
  /* 计时状态恢复（窗口重开 / 刷新后） */
  try{
    const t = await api.timerState();
    if(t && t.running){
      state.timerRunning = true;
      state.seconds = t.elapsed || 0;
      state.timerTaskId = t.projectId+'-'+t.taskId;
      $('#clip').classList.add('is-timing');
      $('#clipState').textContent = '工作中';
      renderClipTimer();
    }
  }catch(_){}
  api.setWindowSize(480, 96);
}
boot();
