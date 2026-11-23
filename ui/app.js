'use strict';
/* ====================== утилиты ====================== */
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const esc = s => String(s ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
const sleep = ms => new Promise(r => setTimeout(r, ms));
const debounce = (fn, ms) => { let t; return (...a) => { clearTimeout(t); t = setTimeout(() => fn(...a), ms); }; };
const enc = encodeURIComponent;
const LOADER_NAMES = { vanilla: 'Vanilla', fabric: 'Fabric', quilt: 'Quilt', forge: 'Forge', neoforge: 'NeoForge' };

function setOffline(v) { $('#offline').hidden = !v; }

async function request(url, opts) {
  let r;
  try { r = await fetch(url, opts); } catch (e) { setOffline(true); throw new Error('Нет связи с лаунчером. Возможно, он был закрыт — запусти его снова.'); }
  setOffline(false);
  let j = {};
  try { j = await r.json(); } catch (e) { /* пустой ответ */ }
  if (!r.ok) throw new Error(j.error || `Ошибка ${r.status}`);
  return j;
}
const api = (name, body) => request('api/' + name, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body || {}) });
const apiGet = (name, params) => request('api/' + name + (params ? '?' + new URLSearchParams(params) : ''));
async function upload(kind, query, file) {
  return request('upload/' + kind + '?' + new URLSearchParams({ ...query, name: file.name }), { method: 'POST', body: file });
}

const plural = (n, a, b, c) => { const m10 = n % 10, m100 = n % 100; return m10 === 1 && m100 !== 11 ? a : (m10 >= 2 && m10 <= 4 && (m100 < 12 || m100 > 14)) ? b : c; };
function fmtDur(s) {
  if (!s) return '—';
  if (s < 60) return 'меньше минуты';
  const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60);
  return h ? `${h} ч${m ? ` ${m} мин` : ''}` : `${m} мин`;
}
function fmtAgo(ts) {
  if (!ts) return 'Ещё не запускали';
  const d = Date.now() / 1000 - ts;
  if (d < 90) return 'Только что';
  if (d < 3600) return `${Math.floor(d / 60)} мин назад`;
  if (d < 86400) return `${Math.floor(d / 3600)} ч назад`;
  const days = Math.floor(d / 86400);
  if (days === 1) return 'Вчера';
  if (days < 14) return `${days} ${plural(days, 'день', 'дня', 'дней')} назад`;
  return new Date(ts * 1000).toLocaleDateString('ru-RU');
}
function fmtSize(b) {
  if (b < 1024) return b + ' Б';
  if (b < 1 << 20) return Math.round(b / 1024) + ' КБ';
  if (b < 1 << 30) return (b / (1 << 20)).toFixed(b < 10 << 20 ? 1 : 0).replace('.', ',') + ' МБ';
  return (b / (1 << 30)).toFixed(1).replace('.', ',') + ' ГБ';
}
function fmtNum(n) {
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 0 : 1).replace('.', ',') + ' млн';
  if (n >= 1e3) return Math.round(n / 1e3) + ' тыс.';
  return String(n);
}

/* ====================== иконки ====================== */
const IC = {
  grid: '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/>',
  compass: '<circle cx="12" cy="12" r="10"/><polygon points="16.2 7.8 14.1 14.1 7.8 16.2 9.9 9.9 16.2 7.8"/>',
  sliders: '<path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/>',
  user: '<path d="M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2"/><circle cx="12" cy="7" r="4"/>',
  plus: '<path d="M5 12h14M12 5v14"/>',
  play: '<polygon points="7 3.5 20 12 7 20.5 7 3.5" fill="currentColor"/>',
  stop: '<rect x="6" y="6" width="12" height="12" rx="2" fill="currentColor"/>',
  folder: '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z"/>',
  trash: '<path d="M3 6h18M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/>',
  copy: '<rect x="8" y="8" width="14" height="14" rx="2"/><path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2"/>',
  download: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M7 10l5 5 5-5M12 15V3"/>',
  upload: '<path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4M17 8l-5-5-5 5M12 3v12"/>',
  search: '<circle cx="11" cy="11" r="8"/><path d="m21 21-4.3-4.3"/>',
  x: '<path d="M18 6 6 18M6 6l12 12"/>',
  check: '<path d="M20 6 9 17l-5-5"/>',
  refresh: '<path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8M21 3v5h-5M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16M8 16H3v5"/>',
  terminal: '<path d="m4 17 6-6-6-6M12 19h8"/>',
  heart: '<path d="M20.8 4.6a5.5 5.5 0 0 0-7.8 0L12 5.7l-1-1.1a5.5 5.5 0 0 0-7.8 7.8l1 1.1L12 21l7.8-7.5 1-1.1a5.5 5.5 0 0 0 0-7.8z"/>',
  globe: '<circle cx="12" cy="12" r="10"/><path d="M2 12h20M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z"/>',
  server: '<rect x="2" y="2" width="20" height="8" rx="2"/><rect x="2" y="14" width="20" height="8" rx="2"/><path d="M6 6h.01M6 18h.01"/>',
  more: '<circle cx="5" cy="12" r="1.2" fill="currentColor"/><circle cx="12" cy="12" r="1.2" fill="currentColor"/><circle cx="19" cy="12" r="1.2" fill="currentColor"/>',
  back: '<path d="M19 12H5M12 19l-7-7 7-7"/>',
  clock: '<circle cx="12" cy="12" r="10"/><path d="M12 6v6l4 2"/>',
  share: '<path d="M4 12v7a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-7M16 6l-4-4-4 4M12 2v14"/>',
  image: '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21"/>',
  shirt: '<path d="M20.4 5.6 16 3a4 4 0 0 1-8 0L3.6 5.6a1 1 0 0 0-.5 1.2l1 3.2a1 1 0 0 0 1 .7H7v9a1 1 0 0 0 1 1h8a1 1 0 0 0 1-1v-9h1.9a1 1 0 0 0 1-.7l1-3.2a1 1 0 0 0-.5-1.2z"/>',
  link: '<path d="M10 13a5 5 0 0 0 7.5.5l3-3a5 5 0 0 0-7-7l-1.7 1.7"/><path d="M14 11a5 5 0 0 0-7.5-.5l-3 3a5 5 0 0 0 7 7l1.7-1.7"/>',
};
const ic = (n, s = 18) => `<svg class="ic" width="${s}" height="${s}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${IC[n]}</svg>`;
const LOGO = '<svg class="logo" viewBox="0 0 32 32" aria-hidden="true"><path d="M16 3l12 7v12l-12 7L4 22V10z" fill="#9b7cff"/><path d="M16 3l12 7-12 7L4 10z" fill="#d4c7ff"/><path d="M16 17v12L4 22V10z" fill="#5b3fd6"/></svg>';

/* ====================== пиксельные значки ====================== */
const PALETTES = [
  ['#2b2250', '#1a1636', '#b79cff', '#7c5cf0', '#efe8ff'], ['#10343a', '#0b2226', '#5fe0c4', '#2aa58f', '#d6fff5'],
  ['#40230f', '#26140a', '#ffb35c', '#e0792b', '#fff0d9'], ['#14301c', '#0c1d11', '#7be08a', '#3aa653', '#e3ffe5'],
  ['#3a1523', '#240c15', '#ff7b9c', '#d83d6a', '#ffe3ea'], ['#14213f', '#0c1428', '#6fa8ff', '#3a6fe0', '#e3eeff'],
  ['#3a3414', '#242008', '#ffd95c', '#d9a81f', '#fff6cf'], ['#1f2a33', '#12191f', '#9fb8c9', '#62808f', '#e8f2f8'],
  ['#2f1740', '#1c0d27', '#e48bff', '#b24ad8', '#fbe6ff'], ['#392010', '#22120a', '#d4a373', '#a0693a', '#f6e6d4'],
];
function rng(seed) {
  let h = 2166136261;
  for (const ch of String(seed)) { h ^= ch.charCodeAt(0); h = Math.imul(h, 16777619); }
  return () => { h += 0x6D2B79F5; let t = Math.imul(h ^ (h >>> 15), 1 | h); t ^= t + Math.imul(t ^ (t >>> 7), 61 | t); return ((t ^ (t >>> 14)) >>> 0) / 4294967296; };
}
const pal = seed => PALETTES[Math.floor(rng('p' + seed)() * PALETTES.length)];
const palVars = p => `--p-bg:${p[0]};--p-bg2:${p[1]};--p-glow:${p[2]}`;
function art(seed) {
  const r = rng(seed), p = pal(seed);
  let cells = '';
  for (let y = 0; y < 8; y++) {
    for (let x = 0; x < 4; x++) {
      const edge = x === 0 || y === 0 || y === 7;
      if (r() < (edge ? 0.58 : 0.2)) continue;
      const v = r(), col = v < 0.5 ? p[2] : v < 0.82 ? p[3] : p[4];
      cells += `<rect x="${x}" y="${y}" width="1" height="1" fill="${col}"/><rect x="${7 - x}" y="${y}" width="1" height="1" fill="${col}"/>`;
    }
  }
  return `<svg class="pix" viewBox="0 0 8 8" shape-rendering="crispEdges" aria-hidden="true">${cells}</svg>`;
}

/* ---- значки сборок: набор, цвет или своя картинка ---- */
const PIX_SEEDS = Array.from({ length: 60 }, (_, i) => 'v' + (i + 1));
const iconVars = it => palVars(pal(it.icon_seed));
function iconArt(it, preview) {
  if (it.icon === 'custom') return `<img class="pix pix-img" src="${preview || `api/insticon?key=${enc(it.key)}&v=${it.icon_v || 0}`}" alt="" data-seed="${esc(it.icon_seed)}">`;
  return art(it.icon_seed);
}
/* Своя картинка: обрезаем до квадрата 256×256 в окне, на сервер уходит готовый PNG. */
async function squareIcon(file) {
  const url = URL.createObjectURL(file);
  try {
    const img = await new Promise((ok, no) => { const i = new Image(); i.onload = () => ok(i); i.onerror = () => no(new Error('Не удалось прочитать картинку')); i.src = url; });
    const w = img.naturalWidth, h = img.naturalHeight;
    if (!w || !h) throw new Error('Пустая картинка');
    const side = Math.min(w, h), c = document.createElement('canvas');
    c.width = c.height = 256;
    c.getContext('2d').drawImage(img, (w - side) / 2, (h - side) / 2, side, side, 0, 0, 256, 256);
    const blob = await new Promise(ok => c.toBlob(ok, 'image/png'));
    if (!blob) throw new Error('Не удалось обработать картинку');
    return { blob, data: c.toDataURL('image/png') };
  } finally { URL.revokeObjectURL(url); }
}
function renderIconPicker() {
  const c = S.ic, prev = $('#s-icon');
  if (!prev) return;
  prev.style.cssText = `width:72px;height:72px;border-radius:18px;${iconVars(c)}`;
  prev.innerHTML = iconArt(c, c.preview);
  const box = $('#icon-picker');
  box.hidden = !c.open;
  if (!c.open) { box.innerHTML = ''; return; }
  const seeds = PIX_SEEDS.includes(c.icon_seed) ? PIX_SEEDS : [c.icon_seed, ...PIX_SEEDS];
  box.innerHTML = `<div class="glyph-grid">${seeds.map(sd => `<button type="button" class="gl ${c.icon === '' && c.icon_seed === sd ? 'on' : ''}" style="${palVars(pal(sd))}" data-act="icon-seed" data-s="${esc(sd)}" aria-pressed="${c.icon === '' && c.icon_seed === sd}" title="Выбрать значок">${art(sd)}</button>`).join('')}</div>`;
}

/* ====================== состояние ====================== */
const S = {
  st: null, tasks: {}, prog: {}, nid: 0,
  lib: { q: '', sort: 'recent', filter: 'all' },
  br: null, mods: null, modq: '', logTimer: null, vp: null, seed: null,
};
const route = () => {
  const h = (location.hash || '#/library').slice(2).split('/');
  return { name: h[0] || 'library', key: h[1] ? decodeURIComponent(h[1]) : null, tab: h[2] || 'overview' };
};
const go = path => { location.hash = '#/' + path; };
const goInst = (key, tab) => go(`i/${enc(key)}${tab ? '/' + tab : ''}`);
const getItem = key => S.st.instances.find(i => i.key === key);
const locals = () => S.st.instances.filter(i => i.kind === 'local');
const curAccount = () => S.st.accounts.find(a => a.uuid === S.st.selected) || S.st.accounts[0] || null;
async function refresh() {
  S.st = await apiGet('state');
  const th = S.st.settings && S.st.settings.theme && typeof S.st.settings.theme === 'object' ? S.st.settings.theme : {};
  if (JSON.stringify(th) !== S.themeApplied) { S.themeApplied = JSON.stringify(th); S.theme = th; applyTheme(th); }
}

/* ====================== всплывающие элементы ====================== */
function toast(text, kind = 'info') {
  const el = document.createElement('div');
  el.className = 'toast ' + kind;
  el.innerHTML = `<span>${esc(text)}</span><button aria-label="Закрыть">${ic('x', 15)}</button>`;
  const box = $('#toasts');
  box.append(el);
  while (box.childElementCount > 4) box.firstChild.remove();
  const kill = () => el.remove();
  $('button', el).onclick = kill;
  setTimeout(kill, kind === 'error' ? 10000 : 5000);
}

function modal({ title, body, foot = '', wide = false, onMount }) {
  closeModal();
  const root = $('#modal-root');
  root.innerHTML = `<div class="backdrop" data-act="modal-bg"><div class="modal ${wide ? 'wide' : ''}" role="dialog" aria-modal="true" aria-label="${esc(title)}">
    <div class="modal-head"><h2>${esc(title)}</h2><button class="btn ghost sq" data-act="modal-close" aria-label="Закрыть">${ic('x')}</button></div>
    <div class="modal-body">${body}</div>${foot ? `<div class="modal-foot">${foot}</div>` : ''}</div></div>`;
  const first = $('input:not([type=checkbox]):not([type=file]), select, textarea', root);
  if (first) first.focus();
  if (onMount) onMount($('.modal', root));
  return $('.modal', root);
}
function closeModal() { $('#modal-root').innerHTML = ''; if (S.msPoll) { clearInterval(S.msPoll); S.msPoll = null; } }
function closePop() { $('#pop-root').innerHTML = ''; }
function popAt(anchor, html, { right = false, up = false } = {}) {
  closePop();
  const root = $('#pop-root');
  root.innerHTML = `<div class="pop" style="visibility:hidden">${html}</div>`;
  const pop = $('.pop', root), a = anchor.getBoundingClientRect(), pw = pop.offsetWidth, ph = pop.offsetHeight;
  let left = right ? a.right - pw : a.left, top = up ? a.top - ph - 8 : a.bottom + 8;
  left = Math.max(8, Math.min(left, innerWidth - pw - 8));
  top = Math.max(8, Math.min(top, innerHeight - ph - 8));
  pop.style.cssText = `left:${left}px;top:${top}px`;
  return pop;
}
function confirmBox({ title, text, ok = 'Подтвердить', danger = false }) {
  return new Promise(resolve => {
    modal({
      title, body: `<p>${text}</p>`,
      foot: `<button class="btn" data-act="modal-close">Отмена</button><button class="btn ${danger ? 'danger' : 'primary'}" id="cb-ok">${esc(ok)}</button>`,
      onMount: m => { $('#cb-ok', m).onclick = () => { closeModal(); resolve(true); }; },
    });
    const bg = $('#modal-root .backdrop');
    const watch = new MutationObserver(() => { if (!$('#modal-root .backdrop')) { watch.disconnect(); resolve(false); } });
    watch.observe($('#modal-root'), { childList: true });
    void bg;
  });
}
function askText({ title, label, value = '', ok = 'Готово' }) {
  return new Promise(resolve => {
    let done = false;
    const finish = v => { if (!done) { done = true; resolve(v); } };
    modal({
      title, body: `<label class="fld"><span>${esc(label)}</span><input type="text" id="at-in" value="${esc(value)}" maxlength="60"></label>`,
      foot: `<button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" id="at-ok">${esc(ok)}</button>`,
      onMount: m => {
        const go_ = () => { const v = $('#at-in', m).value.trim(); if (v) { finish(v); closeModal(); } };
        $('#at-ok', m).onclick = go_;
        $('#at-in', m).onkeydown = e => { if (e.key === 'Enter') go_(); };
        $('#at-in', m).select();
      },
    });
    new MutationObserver((_, o) => { if (!$('#modal-root .backdrop')) { o.disconnect(); finish(null); } }).observe($('#modal-root'), { childList: true });
  });
}

/* ====================== аватары ====================== */
function avatar(acc, cls = '') {
  const skin = acc.type === 'ely' ? `https://skinsystem.ely.by/skins/${enc(acc.name)}.png` : `https://crafatar.com/skins/${acc.uuid}`;
  return `<div class="avatar ${cls}" data-skin="${esc(skin)}"><span>${esc((acc.name || '?')[0].toUpperCase())}</span></div>`;
}
function hydrateAvatars(root = document) {
  $$('.avatar[data-skin]', root).forEach(el => {
    if (el.dataset.done) return;
    el.dataset.done = '1';
    const img = new Image();
    img.onload = () => {
      el.style.backgroundImage = `url("${el.dataset.skin}"), url("${el.dataset.skin}")`;
      el.style.backgroundSize = '800% 800%, 800% 800%';
      el.style.backgroundPosition = '71.4286% 14.2857%, 14.2857% 14.2857%';
      el.classList.add('has-skin');
    };
    img.src = el.dataset.skin;
  });
}
document.addEventListener('error', e => {
  const t = e.target;
  if (t.tagName === 'IMG' && t.dataset.seed) { const w = document.createElement('span'); w.innerHTML = art(t.dataset.seed); t.replaceWith(w.firstChild); }
}, true);

/* ====================== общие куски разметки ====================== */
const loaderChip = it => { const t = it.loader?.type || 'vanilla'; return `<span class="chip ld ld-${esc(t)}">${esc(LOADER_NAMES[t] || t)}</span>`; };
const chips = it => `<div class="chips"><span class="chip">Minecraft ${esc(it.mc_version || '?')}</span>${loaderChip(it)}</div>`;

function playBtn(it, size = '') {
  const k = esc(it.key);
  if (it.phase === 'running') return `<button class="btn stop ${size}" data-act="stop" data-key="${k}">${ic('stop')} Остановить</button>`;
  if (it.phase === 'preparing') return `<button class="btn play ${size}" disabled><span class="spin"></span> Подготовка…</button>`;
  const label = 'Играть';
  return `<button class="btn play ${size}" data-act="play" data-key="${k}">${ic('play')} ${label}</button>`;
}

/* ====================== рейка ====================== */
function renderRail(r) {
  const acc = curAccount();
  const running = S.st.instances.some(i => i.phase === 'running' || i.phase === 'preparing');
  const item = (name, icon, label, on) => `<button class="rail-btn ${on ? 'on' : ''}" data-act="nav" data-to="${name}" title="${label}">${ic(icon, 22)}<span>${label}</span>${name === 'library' && running ? '<i class="dot run" title="Игра запущена"></i>' : name === 'settings' && S.upd && S.upd.available ? '<i class="dot" title="Есть обновление"></i>' : ''}</button>`;
  $('#rail').innerHTML = `${LOGO}
    ${item('library', 'grid', 'Библиотека', r.name === 'library' || r.name === 'i')}
    ${item('discover', 'compass', 'Обзор', r.name === 'discover')}
    ${item('shots', 'image', 'Снимки', r.name === 'shots')}
    ${item('skins', 'shirt', 'Скины', r.name === 'skins').replace('</button>', '<i class="soon-tag">скоро</i></button>')}
    ${item('network', 'globe', 'Сеть', r.name === 'network')}
    ${item('settings', 'sliders', 'Настройки', r.name === 'settings')}
    <div class="grow"></div>
    <button class="rail-btn support" data-act="open-url" data-url="https://boosty.to/kingo0offf" title="Поддержать проект на Boosty">${ic('heart', 22)}<span>Поддержать</span></button>
    <button class="acct-btn ${acc ? '' : 'no-acct'}" data-act="acct-menu" title="${acc ? esc(acc.name) : 'Добавить аккаунт'}">${acc ? avatar(acc) : ic('user', 22)}</button>`;
  hydrateAvatars($('#rail'));
}

/* ====================== библиотека ====================== */
function libItems() {
  let a = [...S.st.instances];
  const { q, sort, filter } = S.lib;
  if (filter !== 'all') a = a.filter(i => i.kind === filter);
  if (q) { const s = q.toLowerCase(); a = a.filter(i => `${i.name} ${i.mc_version} ${i.loader_label}`.toLowerCase().includes(s)); }
  const by = {
    recent: (x, y) => (y.last_played - x.last_played) || x.name.localeCompare(y.name),
    name: (x, y) => x.name.localeCompare(y.name),
    time: (x, y) => y.playtime_s - x.playtime_s || x.name.localeCompare(y.name),
  };
  return a.sort(by[sort]);
}
function cardHTML(it) {
  const k = esc(it.key), running = it.phase === 'running', prep = it.phase === 'preparing';
  const tag = '';

  const live = running ? '<span class="live">Запущена</span>' : prep ? '<span class="live prep">Готовится</span>' : '';
  const meta = fmtAgo(it.last_played);
  const btn = running
    ? `<button class="card-play stop always" data-act="stop" data-key="${k}" title="Остановить">${ic('stop', 18)}</button>`
    : prep ? '' : `<button class="card-play" data-act="play" data-key="${k}" title="Играть">${ic('play', 20)}</button>`;
  return `<article class="card" style="${iconVars(it)}" data-act="open" data-key="${k}" tabindex="0" role="link" aria-label="${esc(it.name)}">
    <div class="card-art">${iconArt(it)}${tag}${live}${btn}</div>
    <div class="card-body"><h3 title="${esc(it.name)}">${esc(it.name)}</h3>${chips(it)}<div class="meta">${ic('clock', 13)}${esc(meta)}</div></div></article>`;
}
function gridHTML() {
  const a = libItems();
  if (!S.st.instances.length) {
    return `<div class="empty">${art('empty-state')}<h3>Здесь появятся твои сборки</h3><p>Создай первую или импортируй готовую с Modrinth.</p>
      <div style="margin-top:18px;display:flex;gap:10px;justify-content:center"><button class="btn primary" data-act="create">${ic('plus')} Новая сборка</button><button class="btn" data-act="nav" data-to="discover">${ic('compass')} Найти сборку</button></div></div>`;
  }
  if (!a.length) return `<div class="empty"><h3>Ничего не нашлось</h3><p>Попробуй другой запрос или смени фильтр.</p></div>`;
  return `<div class="grid">${a.map(cardHTML).join('')}${!S.lib.q ? `<button class="card new-card" data-act="create">${ic('plus', 26)}Новая сборка</button>` : ''}</div>`;
}
function heroHTML() {
  if (!curAccount()) {
    return `<section class="onboard"><div><h2>Начни с аккаунта</h2><p>Войди через Ely.by — после этого любую сборку можно запускать одной кнопкой.</p></div>
      <div style="display:flex;gap:10px;flex-wrap:wrap"><button class="btn primary" data-act="ely-login">Войти через Ely.by</button></div></section>`;
  }
  if (S.lib.q) return '';
  const it = [...S.st.instances].filter(i => i.last_played).sort((a, b) => b.last_played - a.last_played)[0];
  if (!it) return '';
  return `<section class="hero" style="${iconVars(it)}"><div>${iconArt(it)}</div>
    <div><div class="over">Продолжить</div><h2>${esc(it.name)}</h2>${chips(it)}</div>${playBtn(it, 'lg')}</section>`;
}
function libraryHTML() {
  const n = S.st.instances.length;
  return `<div class="page"><div class="page-head"><div><h1>Библиотека</h1><p class="sub">${n ? `${n} ${plural(n, 'сборка', 'сборки', 'сборок')}` : 'Пока пусто'}</p></div>
    <div class="head-actions"><button class="btn" data-act="import">${ic('download')} Импорт</button><button class="btn primary" data-act="create">${ic('plus')} Новая сборка</button></div></div>
    <div id="hero">${heroHTML()}</div>
    <div class="toolbar"><div class="search">${ic('search', 16)}<input type="search" id="lib-q" placeholder="Поиск по сборкам" value="${esc(S.lib.q)}" autocomplete="off"></div>
      <select id="lib-sort" aria-label="Сортировка">${[['recent', 'Недавние'], ['name', 'По имени'], ['time', 'По времени игры']].map(([k, t]) => `<option value="${k}" ${S.lib.sort === k ? 'selected' : ''}>${t}</option>`).join('')}</select></div>
    <div id="grid">${gridHTML()}</div></div>`;
}

/* ====================== страница сборки ====================== */
function instActions(it) {
  return `${playBtn(it, 'lg')}<button class="btn sq" data-act="inst-folder" data-key="${esc(it.key)}" title="Открыть папку">${ic('folder')}</button>
    ${it.kind === 'local' ? `<button class="btn sq" data-act="inst-menu" data-key="${esc(it.key)}" title="Ещё">${ic('more')}</button>` : ''}`;
}
function renderInstance(r) {
  const it = getItem(r.key);
  if (!it) {
    $('#view').innerHTML = `<div class="page"><button class="back" data-act="nav" data-to="library">${ic('back')} Библиотека</button><div class="empty"><h3>Сборка не найдена</h3><p>Возможно, её удалили или сервер сборок сейчас недоступен.</p></div></div>`;
    return;
  }
  const tabs = it.kind === 'local'
    ? [['overview', 'Обзор'], ['mods', 'Моды'], ['add', 'Добавить'], ['worlds', 'Миры'], ['log', 'Журнал'], ['settings', 'Настройки']]
    : [['overview', 'Обзор'], ['mods', 'Моды'], ['worlds', 'Миры'], ['log', 'Журнал']];
  const tab = tabs.some(t => t[0] === r.tab) ? r.tab : 'overview';
  $('#view').innerHTML = `<div class="page" style="${iconVars(it)}">
    <button class="back" data-act="nav" data-to="library">${ic('back')} Библиотека</button>
    <div class="inst-head"><div class="inst-icon">${iconArt(it)}</div>
      <div class="inst-title"><h1>${esc(it.name)}</h1>${chips(it)}</div><div class="inst-actions" id="inst-actions">${instActions(it)}</div></div>
    <div class="tabs" role="tablist">${tabs.map(([k, t]) => `<button class="tab ${k === tab ? 'on' : ''}" role="tab" data-act="tab" data-tab="${k}">${t}${k === 'mods' && it.kind === 'local' ? `<span class="n">${it.mods}</span>` : ''}</button>`).join('')}</div>
    <div id="tabbody"></div></div>`;
  loadTab(it, tab);
}
function loadTab(it, tab) {
  stopLog();
  const el = $('#tabbody');
  if (!el) return;
  const fn = { overview: tabOverview, mods: tabMods, add: tabAdd, worlds: tabWorlds, log: tabLog, settings: tabSettings }[tab];
  Promise.resolve(fn(it, el)).catch(e => { el.innerHTML = `<div class="note warn">${esc(e.message)}</div>`; });
}

async function tabOverview(it, el) {
  const [m, w] = await Promise.all([api('mods.list', { key: it.key }), api('worlds.list', { key: it.key })]);
  if (!$('#tabbody')) return;
  const on = m.mods.filter(x => x.enabled).length;
  const ram = it.kind === 'local' ? (it.ram_mb ? `${it.ram_mb} МБ` : `Общая, ${S.st.settings.ram_mb} МБ`) : 'Общая';
  const vanilla = (it.loader?.type || 'vanilla') === 'vanilla';
  el.innerHTML = `
    ${!curAccount() ? `<div class="note">Чтобы играть, <a href="#" data-act="acct-menu">добавь аккаунт</a> Ely.by.</div>` : ''}
    ${it.kind === 'local' && vanilla && m.mods.length ? '<div class="note warn">В сборке есть моды, но загрузчик не выбран — они не заработают. Выбери Fabric, Quilt, Forge или NeoForge на вкладке «Настройки».</div>' : ''}
    <div class="stats">
      <div class="stat"><span>Последний запуск</span><b>${esc(fmtAgo(it.last_played))}</b></div>
      <div class="stat"><span>Время в игре</span><b>${esc(fmtDur(it.playtime_s))}</b></div>
      <div class="stat"><span>Моды</span><b>${m.mods.length ? `${on} из ${m.mods.length}` : 'нет'}</b></div>
      <div class="stat"><span>Миры</span><b>${w.worlds.length || 'нет'}</b></div></div>
    <div class="cols"><section class="panel"><h2 class="sec">Сведения</h2><dl class="kv">
        <dt>Minecraft</dt><dd>${esc(it.mc_version)}</dd><span></span>
        <dt>Загрузчик</dt><dd>${esc(it.loader_label)}</dd><span></span>
        <dt>Память</dt><dd>${esc(ram)}</dd><span></span>
        ${it.java_args ? `<dt>Аргументы JVM</dt><dd>${esc(it.java_args)}</dd><span></span>` : ''}
        <dt>Папка игры</dt><dd>Миры, моды, настройки</dd><button class="btn sm" data-act="inst-folder" data-key="${esc(it.key)}">Открыть</button></dl></section>
      <section class="panel"><h2 class="sec">Действия</h2><div class="action-list">
        ${it.kind === 'local' ? `<button class="btn" data-act="inst-export" data-key="${esc(it.key)}">${ic('upload')} Экспорт в .zip</button>
          <button class="btn" data-act="inst-dup" data-key="${esc(it.key)}">${ic('copy')} Сделать копию</button>` : `<button class="btn" data-act="inst-folder" data-key="${esc(it.key)}">${ic('folder')} Открыть папку</button>`}
        <button class="btn" data-act="tab" data-tab="log">${ic('terminal')} Журнал игры</button></div></section></div>`;
}

/* ---- моды ---- */
async function tabMods(it, el) {
  const d = await api('mods.list', { key: it.key });
  if (!$('#tabbody')) return;
  S.mods = { ...d, key: it.key, local: it.kind === 'local' };
  S.modq = '';
  el.innerHTML = `
    <div class="log-bar"><div class="search" style="flex:1;min-width:200px">${ic('search', 16)}<input type="search" id="mod-q" placeholder="Фильтр по названию" autocomplete="off"></div>
      ${d.readonly ? '' : `<button class="btn" data-act="mods-check">${ic('refresh')} Проверить обновления</button><span id="upd-all"></span>
        <button class="btn primary" data-act="mods-add">${ic('plus')} Добавить .jar</button><input type="file" id="file-mod" accept=".jar" multiple hidden>`}</div>
    <div class="list" id="modlist"></div>`;
  drawMods();
}
function drawMods() {
  const box = $('#modlist');
  if (!box || !S.mods) return;
  const s = S.modq.toLowerCase(), M = S.mods;
  const rows = M.mods.filter(m => !s || `${m.name} ${m.file}`.toLowerCase().includes(s));
  const upd = new Set(M.updates || []);
  const n = M.updates?.length || 0;
  const ua = $('#upd-all');
  if (ua) ua.innerHTML = n ? `<button class="btn play sm" data-act="mods-update-all">Обновить все (${n})</button>` : '';
  if (!M.mods.length) {
    box.innerHTML = `<div class="empty">${art('no-mods')}<h3>Модов пока нет</h3><p>${M.readonly ? 'В этой сборке нет модов.' : 'Перетащи сюда .jar-файлы или найди моды на вкладке «Добавить».'}</p></div>`;
    return;
  }
  if (!rows.length) { box.innerHTML = '<div class="empty"><h3>Ничего не нашлось</h3></div>'; return; }
  box.innerHTML = rows.map(m => {
    const icon = m.icon ? `<img src="api/modicon?key=${enc(M.key)}&file=${enc(m.file)}" alt="" data-seed="${esc(m.name)}">`
      : m.icon_url ? `<img src="${esc(m.icon_url)}" alt="" loading="lazy" data-seed="${esc(m.name)}">` : art(m.name);
    return `<div class="row ${m.enabled ? '' : 'off'}"><div class="row-ico">${icon}</div>
      <div class="row-main"><b>${esc(m.name)}${m.version ? `<span class="ver">${esc(m.version)}</span>` : ''}${upd.has(m.file) ? '<span class="upd">есть обновление</span>' : ''}</b><small>${esc(m.file)}${m.description ? ' — ' + esc(m.description) : ''}</small></div>
      ${upd.has(m.file) ? `<button class="btn sm play" data-act="mod-update" data-file="${esc(m.file)}">Обновить</button>` : ''}
      ${M.local ? `<label class="sw" title="${m.enabled ? 'Выключить' : 'Включить'}"><input type="checkbox" data-act="mod-toggle" data-file="${esc(m.file)}" ${m.enabled ? 'checked' : ''}><i></i></label>
      <button class="btn ghost sq" data-act="mod-delete" data-file="${esc(m.file)}" title="Удалить">${ic('trash')}</button>` : ''}</div>`;
  }).join('');
}

/* ---- миры ---- */
async function tabWorlds(it, el) {
  const d = await api('worlds.list', { key: it.key });
  if (!$('#tabbody')) return;
  el.innerHTML = `<div class="log-bar"><div class="grow">${d.worlds.length ? `${d.worlds.length} ${plural(d.worlds.length, 'мир', 'мира', 'миров')}` : ''}</div><button class="btn" data-act="open-sub" data-key="${esc(it.key)}" data-sub="saves">${ic('folder')} Папка сохранений</button></div>
    <div class="list">${d.worlds.length ? d.worlds.map(w => `<div class="row"><div class="row-ico">${ic('globe', 22)}</div>
      <div class="row-main"><b>${esc(w.name)}</b><small>${esc(fmtSize(w.size))}, играли ${esc(fmtAgo(w.modified).toLowerCase())}</small></div>
      <button class="btn sm" data-act="world-backup" data-key="${esc(it.key)}" data-name="${esc(w.name)}">Резервная копия</button>
      ${it.kind === 'local' ? `<button class="btn ghost sq" data-act="world-delete" data-key="${esc(it.key)}" data-name="${esc(w.name)}" title="Удалить мир">${ic('trash')}</button>` : ''}</div>`).join('')
      : `<div class="empty">${art('no-worlds')}<h3>Миров пока нет</h3><p>Они появятся после первого запуска игры.</p></div>`}</div>`;
}

/* ---- журнал ---- */
function stopLog() { if (S.logTimer) { clearInterval(S.logTimer); S.logTimer = null; } }
const logClass = l => /\b(ERROR|FATAL|Exception|ОШИБКА)\b|^Caused by/.test(l) ? 'l-err' : /\bWARN/.test(l) ? 'l-warn' : (!l.startsWith('[') && !/^\s|^at /.test(l) && l.length) ? 'l-info' : '';
async function tabLog(it, el) {
  el.innerHTML = `<div class="log-bar"><div class="grow" id="log-src"></div><label class="chk" style="margin:0"><input type="checkbox" id="log-follow" checked> Следить</label>
    <button class="btn sm" data-act="log-copy">${ic('copy', 15)} Копировать</button></div><div class="log" id="logbox" tabindex="0" aria-live="off"></div>`;
  const box = $('#logbox'), src = $('#log-src');
  const add = lines => {
    for (const l of lines) { const d = document.createElement('div'); d.className = logClass(l); d.textContent = l || ' '; box.append(d); }
    while (box.childElementCount > 3500) box.firstChild.remove();
    if ($('#log-follow')?.checked) box.scrollTop = box.scrollHeight;
  };
  if (S.tasks[it.key]) {
    let since = 0;
    const pull = async () => {
      if (!$('#logbox')) return stopLog();
      const d = await api('log', { key: it.key, since });
      since = d.next;
      add(d.lines);
      src.textContent = d.phase === 'running' ? 'Игра запущена, журнал обновляется' : d.phase === 'preparing' ? 'Идёт подготовка к запуску' : 'Журнал последнего запуска';
    };
    await pull();
    S.logTimer = setInterval(() => pull().catch(() => { }), 800);
  } else {
    const d = await api('latest_log', { key: it.key });
    src.textContent = d.lines.length ? 'Файл logs/latest.log с прошлого запуска' : '';
    if (d.lines.length) add(d.lines); else box.innerHTML = '<div class="l-info">Журнал появится после запуска игры.</div>';
  }
}

/* ---- настройки сборки ---- */
async function tabSettings(it, el) {
  S.ic = { key: it.key, icon: it.icon === 'custom' ? 'custom' : '', icon_seed: it.icon_seed, icon_v: it.icon_v || 0, preview: '', open: false };
  el.innerHTML = `<div class="form-narrow"><section class="panel">
    <label class="fld"><span>Название</span><input type="text" id="s-name" value="${esc(it.name)}" maxlength="60"></label>
    <div class="fld"><span>Значок</span><div class="icon-pick"><div class="inst-icon" id="s-icon"></div>
      <div class="icon-side"><div style="display:flex;gap:8px;flex-wrap:wrap"><button class="btn sm" data-act="icon-toggle">Выбрать значок</button><button class="btn sm" data-act="icon-upload">${ic('upload', 15)} Своя картинка</button></div><input type="file" id="icon-file" accept="image/*" hidden><p class="hint">Своя картинка обрежется до квадрата</p></div></div>
      <div id="icon-picker" hidden></div></div>
    <div id="vp"></div>
    <div class="fld"><span>Память, МБ</span><input type="number" id="s-ram" min="0" step="256" value="${it.ram_mb || ''}" placeholder="Общая: ${S.st.settings.ram_mb}">
      <div class="ram-presets">${[2048, 4096, 6144, 8192].map(v => `<button class="btn sm" data-act="ram-preset" data-v="${v}">${v / 1024} ГБ</button>`).join('')}<button class="btn sm ghost" data-act="ram-preset" data-v="">Общая</button></div></div>
    <label class="fld"><span>Аргументы JVM</span><input type="text" id="s-jvm" value="${esc(it.java_args)}" placeholder="например, -XX:+UseG1GC"></label>
    <label class="fld"><span>Описание</span><textarea id="s-notes" maxlength="500">${esc(it.notes)}</textarea></label>
    <div style="display:flex;justify-content:flex-end"><button class="btn primary" data-act="save-settings" data-id="${esc(it.id)}">Сохранить</button></div></section>
    <section class="danger-zone"><div><h2 class="sec" style="margin:0">Удалить сборку</h2><p>Вместе с мирами, модами и настройками. Это нельзя отменить.</p></div><button class="btn danger" data-act="inst-delete" data-key="${esc(it.key)}">${ic('trash')} Удалить</button></section></div>`;
  S.vp = versionPicker($('#vp'), { mc: it.mc_version, loader: it.loader });
  renderIconPicker();
}

/* ====================== выбор версий ====================== */
function versionPicker(root, init) {
  const st = { mc: init.mc || '', type: init.loader?.type || 'fabric', lv: init.loader?.version || '', snap: false, manual: false };
  root.innerHTML = `<label class="fld"><span>Версия Minecraft</span><select id="vp-mc"><option>Загружаю…</option></select><input type="text" id="vp-mc-m" placeholder="Например, 1.20.1" hidden><small id="vp-mc-hint"></small></label>
    <label class="chk"><input type="checkbox" id="vp-snap"> Показывать снапшоты</label>
    <div class="fld"><span>Загрузчик модов</span><div class="seg" id="vp-ld">${['vanilla', 'fabric', 'quilt', 'forge', 'neoforge'].map(k => `<button type="button" data-ld="${k}" class="${k === st.type ? 'on' : ''}">${LOADER_NAMES[k]}</button>`).join('')}</div></div>
    <label class="fld" id="vp-lv-wrap"><span>Версия загрузчика</span><select id="vp-lv"></select><input type="text" id="vp-lv-m" placeholder="Введи версию вручную" hidden><small id="vp-hint"></small></label>`;
  const mcSel = $('#vp-mc', root), mcMan = $('#vp-mc-m', root), mcHint = $('#vp-mc-hint', root);
  const lvSel = $('#vp-lv', root), man = $('#vp-lv-m', root), hint = $('#vp-hint', root);
  let seq = 0;
  async function loadLV() {
    const my = ++seq;
    man.hidden = true; lvSel.hidden = false; hint.textContent = ''; hint.className = '';
    $('#vp-lv-wrap', root).hidden = st.type === 'vanilla';
    if (st.type === 'vanilla') { st.lv = ''; return; }
    if (!st.mc) { st.lv = ''; lvSel.innerHTML = '<option value="">Сначала укажи версию Minecraft</option>'; lvSel.disabled = true; return; }
    lvSel.innerHTML = '<option>Загружаю…</option>'; lvSel.disabled = true;
    try {
      const d = await api('meta.loader_versions', { kind: st.type, mc: st.mc });
      if (my !== seq) return;
      const auto = st.type === 'fabric' || st.type === 'quilt';
      const list = d.versions;
      if (!list.length) throw new Error(`Для Minecraft ${st.mc} нет версий ${LOADER_NAMES[st.type]}. Выбери другую версию игры или загрузчик.`);
      lvSel.innerHTML = (auto ? '<option value="">Последняя (рекомендуется)</option>' : '') + list.map(v => `<option value="${esc(v)}">${esc(v)}</option>`).join('');
      lvSel.value = st.lv && list.includes(st.lv) ? st.lv : (auto ? '' : list[0]);
      st.lv = lvSel.value; lvSel.disabled = false; st.manual = false;
    } catch (e) {
      if (my !== seq) return;
      lvSel.hidden = true; man.hidden = false; man.value = st.lv; st.manual = true;
      hint.textContent = e.message; hint.className = 'err-text';
    }
  }
  async function loadMC() {
    try {
      const d = await api('meta.mc_versions', { snapshots: st.snap });
      const list = d.versions.includes(st.mc) || !st.mc ? d.versions : [st.mc, ...d.versions];
      mcSel.innerHTML = list.map(v => `<option value="${esc(v)}">${esc(v)}</option>`).join('');
      if (!st.mc) st.mc = list[0];
      mcSel.value = st.mc; mcSel.hidden = false; mcMan.hidden = true; mcHint.textContent = '';
    } catch (e) {
      // список недоступен (нет сети?) — даём ввести версию руками, чтобы лаунчер не вставал
      mcSel.hidden = true; mcMan.hidden = false; mcMan.value = st.mc;
      mcHint.textContent = `Не удалось загрузить список версий: ${e.message.replace(/[.\s]+$/, '')}. Введи версию вручную.`; mcHint.className = 'err-text';
    }
    loadLV();
  }
  mcSel.onchange = () => { st.mc = mcSel.value; loadLV(); };
  mcMan.oninput = debounce(() => { st.mc = mcMan.value.trim(); loadLV(); }, 500);
  lvSel.onchange = () => { st.lv = lvSel.value; };
  man.oninput = () => { st.lv = man.value.trim(); };
  $('#vp-snap', root).onchange = e => { st.snap = e.target.checked; loadMC(); };
  $('#vp-ld', root).onclick = e => {
    const b = e.target.closest('[data-ld]'); if (!b) return;
    st.type = b.dataset.ld; st.lv = '';
    $$('#vp-ld button', root).forEach(x => x.classList.toggle('on', x === b));
    loadLV();
  };
  loadMC();
  // читаем поля в момент вызова: ручной ввод применяется с задержкой, а «Создать» могут нажать сразу
  return {
    get: () => ({
      mc_version: mcMan.hidden ? st.mc : mcMan.value.trim(),
      loader: { type: st.type, version: st.type === 'vanilla' ? '' : (man.hidden ? st.lv : man.value.trim()) },
    }),
  };
}

/* ====================== создание и импорт ====================== */
function openCreate(tab = 'new') {
  modal({
    title: 'Новая сборка', wide: true,
    body: `<div class="seg" style="margin-bottom:18px" id="cr-tabs"><button data-ct="new" class="${tab === 'new' ? 'on' : ''}">Создать</button><button data-ct="import" class="${tab === 'import' ? 'on' : ''}">Импорт из файла</button></div><div id="cr-body"></div>`,
    onMount: m => {
      $('#cr-tabs', m).onclick = e => { const b = e.target.closest('[data-ct]'); if (b) { $$('#cr-tabs button', m).forEach(x => x.classList.toggle('on', x === b)); drawCreate(b.dataset.ct); } };
      drawCreate(tab);
    },
  });
}
function drawCreate(tab) {
  const body = $('#cr-body');
  if (tab === 'import') {
    body.innerHTML = `<div class="dropzone" id="imp-zone"><p style="margin-bottom:12px">Выбери или перетащи файл сборки</p><button class="btn primary" data-act="imp-pick">${ic('download')} Выбрать файл</button>
      <input type="file" id="imp-file" accept=".mrpack,.zip" hidden><p class="hint" style="margin-top:14px">.mrpack с Modrinth, экспорт Voxel или .zip из Prism / MultiMC</p></div>`;
    return;
  }
  body.innerHTML = `<label class="fld"><span>Название</span><input type="text" id="cr-name" placeholder="Например, Выживание" maxlength="60"></label>
    <div id="vp"></div>
    <details class="adv"><summary>Дополнительно</summary><div class="two"><label class="fld"><span>Память, МБ</span><input type="number" id="cr-ram" min="0" step="256" placeholder="Общая"></label>
    <label class="fld"><span>Аргументы JVM</span><input type="text" id="cr-jvm"></label></div></details>
    <div class="modal-foot" style="padding:6px 0 10px"><button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" data-act="create-go">Создать</button></div>`;
  S.vp = versionPicker($('#vp'), { mc: '', loader: { type: 'fabric', version: '' } });
  $('#cr-name').focus();
}
async function importFile(file) {
  if (!file) return;
  closeModal();
  toast(`Импортирую «${file.name}»…`);
  try {
    const d = await upload('import', {}, file);
    await refresh();
    toast(`Сборка «${d.instance.name}» добавлена`, 'ok');
    goInst(d.instance.key);
  } catch (e) { toast(e.message, 'error'); }
}

/* ====================== обзор Modrinth ====================== */
function discoverHTML() {
  return `<div class="page"><div class="page-head"><div><h1>Обзор</h1><p class="sub">Моды, шейдеры, ресурспаки и готовые сборки с Modrinth и CurseForge</p></div></div><div id="browse"></div></div>`;
}
/* Теги Modrinth: подпись по-русски, значение для поиска по-английски */
const BR_TAGS = {
  mod: [['adventure', 'Приключения'], ['optimization', 'Оптимизация'], ['technology', 'Технологии'], ['magic', 'Магия'], ['worldgen', 'Генерация мира'], ['mobs', 'Мобы'], ['decoration', 'Декор'], ['equipment', 'Снаряжение'], ['food', 'Еда'], ['storage', 'Хранение'], ['transportation', 'Транспорт'], ['utility', 'Утилиты'], ['library', 'Библиотеки'], ['management', 'Управление'], ['social', 'Социальное'], ['economy', 'Экономика'], ['game-mechanics', 'Механики'], ['minigame', 'Мини-игры'], ['cursed', 'Проклятое']],
  resourcepack: [['combat', 'Бой'], ['decoration', 'Декор'], ['modded', 'Для модов'], ['realistic', 'Реализм'], ['simplistic', 'Минимализм'], ['themed', 'Тематические'], ['tweaks', 'Твики'], ['utility', 'Утилиты'], ['vanilla-like', 'Как ваниль'], ['cursed', 'Проклятое']],
  shader: [['cartoon', 'Мультяшные'], ['fantasy', 'Фэнтези'], ['realistic', 'Реалистичные'], ['semi-realistic', 'Полуреалистичные'], ['vanilla-like', 'Как ваниль'], ['cursed', 'Проклятое']],
  modpack: [['adventure', 'Приключения'], ['challenging', 'Сложные'], ['combat', 'Бой'], ['kitchen-sink', 'Всё и сразу'], ['lightweight', 'Лёгкие'], ['magic', 'Магия'], ['multiplayer', 'Мультиплеер'], ['optimization', 'Оптимизация'], ['quests', 'Квесты'], ['technology', 'Технологии']],
};
const LIMITS = [5, 10, 15, 20, 50, 100];
const lsGet = k => { try { return localStorage.getItem(k); } catch { return null; } };
const lsSet = (k, v) => { try { localStorage.setItem(k, v); } catch { } };
function drawLimit(open) {
  const B = S.br, box = $('#br-lim'); if (!B || !box) return;
  box.innerHTML = `<button type="button" class="vsel-btn" data-act="lim-toggle" aria-haspopup="listbox" aria-expanded="${!!open}">Вид: <b>${B.limit}</b><svg class="ic" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="m6 9 6 6 6-6"/></svg></button>`
    + (open ? `<div class="vsel-menu" role="listbox">${LIMITS.map(n => `<button type="button" role="option" class="${n === B.limit ? 'on' : ''}" data-act="lim-set" data-n="${n}">${n}</button>`).join('')}</div>` : '');
}
function drawTags() {
  const B = S.br, box = $('#br-tags');
  if (!B || !box) return;
  const list = B.src === 'modrinth' ? BR_TAGS[B.type] || [] : [];
  box.hidden = !list.length;
  box.innerHTML = list.map(([k, t]) => `<button type="button" class="tag ${B.tags.has(k) ? 'on' : ''}" data-act="br-tag" data-t="${k}" aria-pressed="${B.tags.has(k)}">${t}</button>`).join('')
    + (B.tags.size ? `<button type="button" class="tag clear" data-act="br-tags-clear">${ic('x', 13)} Сбросить (${B.tags.size})</button>` : '');
}
/* md-begin */
/* Описание мода приходит в Markdown. Весь текст сначала экранируется, потом разрешаются только свои теги,
   а ссылки и картинки только https. Так чужое описание не может ничего выполнить. */
const safeHttps = u => /^https:\/\/[^\s"'<>]+$/i.test(u);
function mdInline(t) {
  const keep = [];
  const hold = h => '\u0002' + (keep.push(h) - 1) + '\u0002';
  t = t.replace(/`([^`\n]+)`/g, (_, c) => hold(`<code>${c}</code>`));
  t = t.replace(/!\[([^\]]*)\]\(\s*(\S+?)(?:\s+&quot;[^)]*?&quot;)?\s*\)/g, (m, alt, u) => safeHttps(u) ? hold(`<img src="${u}" alt="${alt}" loading="lazy">`) : '');
  t = t.replace(/\[([^\]]+)\]\(\s*(\S+?)(?:\s+&quot;[^)]*?&quot;)?\s*\)/g, (m, txt, u) => safeHttps(u) ? `<a href="#" data-act="open-link" data-url="${u}">${txt}</a>` : txt);
  t = t.replace(/\*\*(.+?)\*\*|__(.+?)__/g, (m, a, b) => `<strong>${a || b}</strong>`);
  t = t.replace(/(^|[^*\w])\*(?!\s)(.+?)\*(?!\w)/g, '$1<em>$2</em>');
  t = t.replace(/(^|[^\w])_(?!\s)(.+?)_(?!\w)/g, '$1<em>$2</em>');
  t = t.replace(/~~(.+?)~~/g, '<del>$1</del>');
  return t.replace(/\u0002(\d+)\u0002/g, (_, i) => keep[+i]);
}
function mdHtml(src) {
  let s = String(src || '').replace(/\r\n?/g, '\n');
  const blocks = [];
  s = s.replace(/```[^\n]*\n([\s\S]*?)```/g, (_, c) => '\n\u0001' + (blocks.push(`<pre><code>${esc(c.replace(/\n$/, ''))}</code></pre>`) - 1) + '\u0001\n');
  s = s.replace(/<!--[\s\S]*?-->/g, '');
  s = s.replace(/<img\b[^>]*>/gi, tag => { const m = /\bsrc\s*=\s*["']([^"']+)["']/i.exec(tag); return m && safeHttps(m[1]) ? `![](${m[1]})` : ''; });
  s = s.replace(/<h([1-6])\b[^>]*>([\s\S]*?)<\/h\1>/gi, (_, n, t) => `\n\n${'#'.repeat(+n)} ${t.replace(/<[^>]*>/g, '').trim()}\n\n`);
  s = s.replace(/<br\s*\/?>/gi, '\n').replace(/<\/(?:p|div|li|tr|details|summary|center|ul|ol|table|blockquote)>/gi, '\n\n');
  s = s.replace(/<\/?[a-zA-Z][^>]*>/g, '');
  s = esc(s);
  const lines = s.split('\n'), out = [];
  let i = 0, para = [];
  const flush = () => { if (para.length) { out.push(`<p>${mdInline(para.join(' '))}</p>`); para = []; } };
  const cells = l => l.trim().replace(/^\||\|$/g, '').split('|').map(c => c.trim());
  const item = /^\s*([-*+]|\d+[.)])\s+/;
  while (i < lines.length) {
    const t = lines[i].trim();
    let m;
    if (!t) { flush(); i++; continue; }
    if ((m = /^\u0001(\d+)\u0001$/.exec(t))) { flush(); out.push(blocks[+m[1]]); i++; continue; }
    if ((m = /^(#{1,6})\s+(.+?)\s*#*$/.exec(t))) { flush(); out.push(`<h${m[1].length}>${mdInline(m[2])}</h${m[1].length}>`); i++; continue; }
    if (/^([-*_])(\s*\1){2,}$/.test(t)) { flush(); out.push('<hr>'); i++; continue; }
    if (/^&gt;/.test(t)) { flush(); const q = []; while (i < lines.length && /^\s*&gt;/.test(lines[i])) q.push(lines[i++].replace(/^\s*&gt;\s?/, '')); out.push(`<blockquote>${mdInline(q.join(' '))}</blockquote>`); continue; }
    if (item.test(lines[i])) {
      flush(); const ord = /^\d/.test(t), items = [];
      while (i < lines.length && item.test(lines[i])) items.push(lines[i++].replace(item, ''));
      const tag = ord ? 'ol' : 'ul';
      out.push(`<${tag}>${items.map(x => `<li>${mdInline(x)}</li>`).join('')}</${tag}>`); continue;
    }
    if (t.includes('|') && i + 1 < lines.length && /^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(lines[i + 1])) {
      flush(); const head = cells(lines[i]); i += 2; const rows = [];
      while (i < lines.length && lines[i].trim() && lines[i].includes('|')) rows.push(cells(lines[i++]));
      out.push(`<div class="tbl"><table><thead><tr>${head.map(c => `<th>${mdInline(c)}</th>`).join('')}</tr></thead><tbody>${rows.map(r => `<tr>${r.map(c => `<td>${mdInline(c)}</td>`).join('')}</tr>`).join('')}</tbody></table></div>`);
      continue;
    }
    para.push(t); i++;
  }
  flush();
  return out.join('');
}
/* md-end */
/* html-begin */
/* Описание на CurseForge приходит как HTML. Разбираем его в «неживом» документе (DOMParser: скрипты не запускаются,
   картинки не грузятся), потом заново собираем строку только из разрешённых тегов. Из атрибутов остаются
   только https-адреса у ссылок и картинок, всё остальное (onclick, style, iframe, формы) выбрасывается. */
const HTML_OK = new Set(['p', 'br', 'strong', 'b', 'em', 'i', 'u', 's', 'del', 'ul', 'ol', 'li', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'blockquote', 'pre', 'code', 'hr', 'table', 'thead', 'tbody', 'tr', 'th', 'td', 'a', 'img', 'sub', 'sup']);
const HTML_DROP = new Set(['script', 'style', 'iframe', 'object', 'embed', 'form', 'input', 'button', 'textarea', 'select', 'link', 'meta', 'svg', 'math', 'video', 'audio', 'canvas', 'noscript', 'template', 'head', 'title']);
function htmlSafe(src) {
  let doc;
  try { doc = new DOMParser().parseFromString(String(src || ''), 'text/html'); } catch { return ''; }
  const walk = (node, depth) => {
    if (depth > 60) return '';
    let out = '';
    for (const n of node.childNodes) {
      if (n.nodeType === 3) { out += esc(n.nodeValue); continue; }
      if (n.nodeType !== 1) continue;
      const t = n.tagName.toLowerCase();
      if (HTML_DROP.has(t)) continue;
      const inner = walk(n, depth + 1);
      if (!HTML_OK.has(t)) { out += inner; continue; }
      if (t === 'br' || t === 'hr') { out += `<${t}>`; continue; }
      if (t === 'img') { const u = n.getAttribute('src') || ''; if (safeHttps(u)) out += `<img src="${esc(u)}" alt="${esc(n.getAttribute('alt') || '')}" loading="lazy">`; continue; }
      if (t === 'a') {
        const u = (n.getAttribute('href') || '').trim();
        out += safeHttps(u) ? `<a href="#" data-act="open-link" data-url="${esc(u)}">${inner}</a>` : inner;
        continue;
      }
      if (t === 'table') { out += `<div class="tbl"><table>${inner}</table></div>`; continue; }
      out += `<${t}>${inner}</${t}>`;
    }
    return out;
  };
  return walk(doc.body, 0);
}
/* html-end */

/* ---- страница мода (Modrinth) ---- */
let _tagRu;
const tagName = c => (_tagRu ||= Object.fromEntries(Object.values(BR_TAGS).flat()))[c] || c;
const fmtDate = iso => { const d = new Date(iso); return isNaN(d) ? '' : d.toLocaleDateString('ru-RU', { year: 'numeric', month: 'short', day: 'numeric' }); };
const SIDE = { required: 'нужен', optional: 'по желанию', unsupported: 'не нужен', unknown: '—' };
async function openProject(h) {
  const B = S.br, D = B.detail = { h, tab: 'desc', p: null, v: null, err: '', verr: '' };
  try { history.pushState({ brDetail: 1 }, ''); D.pushed = true; } catch { }
  drawDetail();
  $('#br-detail')?.scrollIntoView?.({ block: 'start' });
  try { const p = await api(B.src + '.project', { id: h.project_id }); if (B.detail !== D) return; D.p = p; }
  catch (e) { if (B.detail !== D) return; D.err = e.message; }
  drawDetail();
}
async function loadVersions() {
  const B = S.br, D = B.detail; if (!D || D.v) return;
  try { const d = await api(B.src + '.versions', { id: D.h.project_id, mc: B.mc, loader: B.type === 'mod' ? B.loader : '', type: B.type }); if (B.detail !== D) return; D.v = d.versions; }
  catch (e) { if (B.detail !== D) return; D.verr = e.message; }
  drawDetail();
}
function drawDetail() {
  const B = S.br, D = B && B.detail, box = $('#br-detail'), list = $('#br-list');
  if (!box || !list) return;
  if (!D) { box.hidden = true; box.innerHTML = ''; list.hidden = false; return; }
  list.hidden = true; box.hidden = false;
  const h = D.h, p = D.p, id = h.project_id;
  const done = B.installed.has(id), busy = B.busy.has(id);
  const label = B.type === 'modpack' ? 'Создать сборку' : B.ctx ? 'Установить' : 'Установить…';
  const siteUrl = B.src === 'curseforge' ? (h.site_url || (p && p.site_url) || '') : 'https://modrinth.com/project/' + id;
  const btn = h.blocked ? `<button class="btn" data-act="open-url" data-url="${esc(h.site_url)}" title="Автор запретил установку из сторонних лаунчеров">${ic('link', 16)} Скачать на сайте</button>` : busy ? '<button class="btn primary" disabled><span class="spin"></span> Ставлю…</button>'
    : done ? `<button class="btn" disabled>${ic('check', 16)} Установлено</button>`
      : `<button class="btn primary" data-act="br-install" data-pid="${esc(id)}">${ic('download', 16)} ${label}</button>`;
  const chips = ((p && p.categories) || h.categories || []).map(c => `<span class="chip">${esc(tagName(c))}</span>`).join('');
  const tabs = [['desc', 'Описание']].concat(p && p.gallery.length ? [['gallery', `Галерея (${p.gallery.length})`]] : [], [['versions', 'Версии']]);
  const spin = '<div class="empty"><span class="spin"></span></div>';
  let body = '';
  if (D.tab === 'desc') {
    if (D.err) body = `<div class="empty"><h3>Не получилось открыть страницу</h3><p>${esc(D.err)}</p></div>`;
    else if (!p) body = spin;
    else {
      const link = (u, t) => u ? `<button class="btn sm" data-act="open-link" data-url="${esc(u)}">${ic('link', 13)} ${t}</button>` : '';
      const links = [link(p.links.source, 'Исходный код'), link(p.links.issues, 'Ошибки'), link(p.links.wiki, 'Вики'), link(p.links.discord, 'Discord')].join('');
      const gv = p.game_versions.slice(-8).reverse();
      body = `<div class="proj-cols"><div class="md panel">${(p.body_html ? htmlSafe(p.body_html) : mdHtml(p.body)) || '<p class="hint">У проекта нет описания.</p>'}</div>
        <aside class="proj-side">
          <div class="panel"><h4>Совместимость</h4>${p.loaders.length ? `<div class="chips">${p.loaders.map(l => `<span class="chip">${esc(LOADER_NAMES[l] || l)}</span>`).join('')}</div>` : ''}${gv.length ? `<div class="chips" style="margin-top:8px">${gv.map(v => `<span class="chip">${esc(v)}</span>`).join('')}</div>` : ''}
            ${p.project_type === 'mod' && p.client_side ? `<p class="hint" style="margin-top:10px">Клиент: ${SIDE[p.client_side] || '—'}<br>Сервер: ${SIDE[p.server_side] || '—'}</p>` : ''}</div>
          ${links ? `<div class="panel"><h4>Ссылки</h4><div class="chips">${links}</div></div>` : ''}
          <div class="panel"><h4>Сведения</h4><p class="hint">${p.license ? 'Лицензия: ' + esc(p.license) + '<br>' : ''}${p.published ? 'Создан: ' + esc(fmtDate(p.published)) + '<br>' : ''}${p.updated ? 'Обновлён: ' + esc(fmtDate(p.updated)) : ''}</p></div>
        </aside></div>`;
    }
  } else if (D.tab === 'gallery') {
    body = `<div class="gallery">${p.gallery.map(g => `<figure><button type="button" class="shot" data-act="br-img" data-url="${esc(g.url)}" aria-label="Открыть: ${esc(g.title || 'картинка')}"><img src="${esc(g.url)}" alt="" loading="lazy"></button>${g.title ? `<figcaption>${esc(g.title)}</figcaption>` : ''}</figure>`).join('')}</div>`;
  } else {
    if (D.verr) body = `<div class="empty"><h3>Не получилось загрузить версии</h3><p>${esc(D.verr)}</p></div>`;
    else if (!D.v) body = spin;
    else {
      const note = B.ctx ? `Показаны версии для ${esc(B.mc)}${B.loader ? ' и ' + esc(LOADER_NAMES[B.loader] || B.loader) : ''}. Установка берёт последнюю подходящую.` : 'Установка берёт последнюю подходящую версию.';
      const TYPE = { release: ['Релиз', 'ok'], beta: ['Бета', 'warn'], alpha: ['Альфа', 'bad'] };
      body = `<p class="hint" style="margin-bottom:10px">${note}</p>` + (D.v.length ? `<div class="vers">${D.v.map(v => {
        const t = TYPE[v.version_type] || TYPE.release, gv = v.game_versions.slice(-4);
        return `<details class="ver"><summary><span class="vt ${t[1]}">${t[0]}</span><b>${esc(v.version_number || v.name)}</b><span class="vm">${gv.map(x => `<span class="chip">${esc(x)}</span>`).join('')}${v.game_versions.length > 4 ? `<span class="chip">+${v.game_versions.length - 4}</span>` : ''}${v.loaders.map(l => `<span class="chip">${esc(LOADER_NAMES[l] || l)}</span>`).join('')}</span><span class="vd">${esc(fmtDate(v.date_published))} · ${ic('download', 12)} ${esc(fmtNum(v.downloads))}</span></summary>
          <div class="md vlog">${v.changelog ? mdHtml(v.changelog) : '<p class="hint">Описания изменений нет.</p>'}</div></details>`;
      }).join('')}</div>` : '<div class="empty"><h3>Подходящих версий нет</h3><p>Для выбранной версии игры и загрузчика этого проекта нет.</p></div>');
    }
  }
  box.innerHTML = `<button class="btn ghost sm" data-act="br-back">${ic('back', 14)} Назад к списку</button>
    <div class="proj-head"><div class="proj-icon">${h.icon_url ? `<img src="${esc(h.icon_url)}" alt="">` : art(id)}</div>
      <div class="proj-info"><h2>${esc(h.title)}<span class="by">${esc(h.author)}</span></h2><p>${esc((p && p.description) || h.description)}</p>
        <div class="proj-stats"><span>${ic('download', 14)} ${esc(fmtNum((p && p.downloads) || h.downloads))} скачиваний</span>${p ? `<span>${ic('heart', 14)} ${esc(fmtNum(p.followers))} подписчиков</span>` : ''}</div>
        <div class="chips">${chips}</div></div>
      <div class="proj-act">${btn}${siteUrl ? `<button class="btn ghost sm" data-act="open-url" data-url="${esc(siteUrl)}">${ic('link', 13)} На сайте</button>` : ''}</div></div>
    <div class="seg proj-tabs">${tabs.map(([k, t]) => `<button data-act="br-tab" data-t="${k}" class="${D.tab === k ? 'on' : ''}">${t}</button>`).join('')}</div>
    <div class="proj-body">${body}</div>`;
}
function tabAdd(it, el) { browseInto(el, it); }
function browseInto(el, it) {
  const B = S.br = {
    ctx: it ? it.key : null, it, src: 'modrinth', type: 'mod', q: '', sort: 'relevance', mc: it ? it.mc_version : '', loader: it ? (it.loader?.type || '') : '',
    hits: [], total: 0, loading: false, err: '', installed: new Set(), busy: new Set(), seq: 0, page: 0, tags: new Set(), limit: LIMITS.includes(+lsGet('br-limit')) ? +lsGet('br-limit') : 20,
  };
  const types = [['mod', 'Моды'], ['resourcepack', 'Ресурспаки'], ['shader', 'Шейдеры']].concat(it ? [] : [['modpack', 'Сборки']]);
  const vanilla = it && (it.loader?.type || 'vanilla') === 'vanilla';
  el.innerHTML = `<div id="br-list"><div class="filters" style="margin-bottom:10px"><div class="seg" id="br-src"><button data-act="br-src" data-s="modrinth" class="on">Modrinth</button><button data-act="br-src" data-s="curseforge">CurseForge</button></div></div>
    <div class="search big">${ic('search', 18)}<input type="search" id="br-q" placeholder="Искать на Modrinth" autocomplete="off"></div>
    <div class="filters"><div class="seg" id="br-types">${types.map(([k, t]) => `<button data-act="br-type" data-t="${k}" class="${k === 'mod' ? 'on' : ''}">${t}</button>`).join('')}</div><span style="flex:1"></span>
      ${it ? `<span class="chip">для ${esc(it.mc_version)}</span>${loaderChip(it)}`
      : `<select id="br-mc" aria-label="Версия Minecraft"><option value="">Любая версия</option></select><select id="br-ld" aria-label="Загрузчик"><option value="">Любой загрузчик</option>${['fabric', 'quilt', 'forge', 'neoforge'].map(k => `<option value="${k}">${LOADER_NAMES[k]}</option>`).join('')}</select>`}
      <div class="vsel" id="br-lim"></div>
      <select id="br-sort" aria-label="Сортировка">${[['relevance', 'По релевантности'], ['downloads', 'По загрузкам'], ['newest', 'Новые'], ['updated', 'Обновлённые']].map(([k, t]) => `<option value="${k}">${t}</option>`).join('')}</select></div>
    <div class="tags" id="br-tags"></div>
    ${vanilla ? '<div class="note warn">У сборки не выбран загрузчик, поэтому моды установить нельзя. Выбери Fabric, Quilt, Forge или NeoForge на вкладке «Настройки». Ресурспаки и шейдеры ставятся и так.</div>' : ''}
    <div class="pager" id="br-pg-top"></div><div class="results" id="br-results"></div><div class="pager" id="br-pg-bot"></div></div><div id="br-detail" hidden></div>`;
  if (it) api('content.installed', { key: it.key }).then(d => { B.installed = new Set(d.project_ids); drawHits(); }).catch(() => { });
  else api('meta.mc_versions', { snapshots: false }).then(d => { $('#br-mc').insertAdjacentHTML('beforeend', d.versions.slice(0, 60).map(v => `<option>${esc(v)}</option>`).join('')); }).catch(() => { });
  drawTags();
  drawLimit();
  brSearch(true);
}
async function brSearch(reset) {
  const B = S.br; if (!B) return;
  const my = ++B.seq;
  if (reset) { B.page = 0; B.total = 0; }
  B.hits = [];
  B.loading = true; B.err = ''; drawHits();
  try {
    const d = await api(B.src + '.search', { query: B.q, type: B.type, mc: B.mc, loader: B.type === 'mod' || B.type === 'modpack' ? B.loader : '', sort: B.sort, offset: B.page * brLimit(B), limit: brLimit(B), tags: B.src === 'modrinth' ? [...B.tags] : [] });
    if (my !== B.seq) return;
    B.hits = d.hits; B.total = d.total;
  } catch (e) { if (my !== B.seq) return; B.err = e.message; }
  B.loading = false; drawHits();
}
/* страницы: у CurseForge больше 50 за раз не бывает */
const brLimit = B => B.src === 'curseforge' ? Math.min(B.limit, 50) : B.limit;
function pageList(cur, n) {
  const set = new Set([0, n - 1, cur - 1, cur, cur + 1]);
  if (cur < 3) { set.add(1); set.add(2); set.add(3); }
  if (cur > n - 4) { set.add(n - 2); set.add(n - 3); set.add(n - 4); }
  const out = []; let prev = -1;
  for (const i of [...set].filter(i => i >= 0 && i < n).sort((a, b) => a - b)) { if (i - prev > 1) out.push(null); out.push(i); prev = i; }
  return out;
}
function drawPager() {
  const B = S.br; if (!B) return;
  const n = Math.ceil(B.total / brLimit(B)), cur = B.page;
  const html = n < 2 ? '' : `<button type="button" class="pg arrow" data-act="br-page" data-n="${cur - 1}" ${cur === 0 ? 'disabled' : ''} aria-label="Предыдущая страница">${ic('back', 14)}</button>`
    + pageList(cur, n).map(i => i === null ? '<span class="pg gap">…</span>' : `<button type="button" class="pg ${i === cur ? 'on' : ''}" data-act="br-page" data-n="${i}" ${i === cur ? 'aria-current="page"' : ''}>${i + 1}</button>`).join('')
    + `<button type="button" class="pg arrow next" data-act="br-page" data-n="${cur + 1}" ${cur >= n - 1 ? 'disabled' : ''} aria-label="Следующая страница">${ic('back', 14)}</button>`;
  for (const id of ['#br-pg-top', '#br-pg-bot']) { const b = $(id); if (b) b.innerHTML = html; }
}
function drawHits() {
  const B = S.br, box = $('#br-results');
  if (!B || !box) return;
  if (B.err) {
    const key = /ключ API CurseForge/.test(B.err);
    box.innerHTML = `<div class="empty"><h3>${key ? 'CurseForge пока недоступен' : 'Не получилось загрузить каталог'}</h3><p>${esc(B.err)}</p><div style="margin-top:14px;display:flex;gap:10px;justify-content:center;flex-wrap:wrap">${key ? `<button class="btn primary" data-act="nav" data-to="settings">${ic('sliders')} Открыть настройки</button><button class="btn" data-act="open-url" data-url="https://console.curseforge.com/">${ic('link', 14)} Получить ключ</button>` : ''}<button class="btn" data-act="br-retry">${ic('refresh')} Повторить</button></div></div>`; drawPager(); return; }
  if (!B.hits.length) { box.innerHTML = B.loading ? '<div class="empty"><span class="spin"></span><p style="margin-top:10px">Ищу…</p></div>' : '<div class="empty"><h3>Ничего не найдено</h3><p>Попробуй другой запрос или сбрось фильтры.</p></div>'; drawPager(); return; }
  box.innerHTML = B.hits.map(h => {
    const done = B.installed.has(h.project_id), busy = B.busy.has(h.project_id);
    const label = B.type === 'modpack' ? 'Создать сборку' : B.ctx ? 'Установить' : 'Установить…';
    const btn = h.blocked ? `<button class="btn" data-act="open-url" data-url="${esc(h.site_url)}" title="Автор запретил установку из сторонних лаунчеров">${ic('link', 16)} Скачать на сайте</button>` : busy ? '<button class="btn primary" disabled><span class="spin"></span> Ставлю…</button>'
      : done ? `<button class="btn" disabled>${ic('check', 16)} Установлено</button>`
        : `<button class="btn primary" data-act="br-install" data-pid="${esc(h.project_id)}">${ic('download', 16)} ${label}</button>`;
    const open = ` data-act="br-open" data-pid="${esc(h.project_id)}"`;
    return `<article class="hit"><div class="hit-icon ${open ? 'link' : ''}"${open}>${h.icon_url ? `<img src="${esc(h.icon_url)}" alt="" loading="lazy" data-seed="${esc(h.project_id)}">` : art(h.project_id)}</div>
      <div class="hit-main"><h3>${open ? `<a href="#" class="plink"${open}>${esc(h.title)}</a>` : esc(h.title)}<span class="by">${esc(h.author)}</span></h3><p>${esc(h.description)}</p>
        <div class="chips">${h.categories.map(c => `<span class="chip">${esc(c)}</span>`).join('')}<button class="btn ghost sm site" data-act="open-url" data-url="${B.src === 'curseforge' ? esc(h.site_url) : 'https://modrinth.com/project/' + esc(h.project_id)}">${ic('link', 13)} На сайте</button></div></div>
      <div class="hit-side">${btn}<span class="dl">${ic('download', 13)} ${esc(fmtNum(h.downloads))}</span></div></article>`;
  }).join('');
  if (B.detail) drawDetail();
  drawPager();
}
async function installTo(key, h, type) {
  const B = S.br;
  B.busy.add(h.project_id); drawHits();
  try {
    const d = await api(B.src + '.install', { key, project_id: h.project_id, type, title: h.title, icon_url: h.icon_url, site_url: h.site_url || '' });
    const extra = d.installed.length - 1;
    toast(`Установлено: ${h.title}${extra > 0 ? ` и ещё ${extra} ${plural(extra, 'зависимость', 'зависимости', 'зависимостей')}` : ''}`, 'ok');
    if (B.ctx === key) B.installed.add(h.project_id);
    refresh().then(() => { const t = $('.tab .n'); const it = getItem(key); if (t && it) t.textContent = it.mods; });
  } catch (e) { toast(e.message, 'error'); } finally { B.busy.delete(h.project_id); drawHits(); }
}
function chooseInstance(h, type) {
  const list = locals();
  if (!list.length) { toast('Сначала создай сборку, в которую поставить это.', 'error'); return; }
  const bad = i => type === 'mod' && (i.loader?.type || 'vanilla') === 'vanilla';
  const firstOk = list.findIndex(i => !bad(i));
  modal({
    title: `Установить «${h.title}»`,
    body: `${firstOk < 0 ? '<div class="note warn">Моды работают только с загрузчиком. Ни у одной сборки он не выбран — открой сборку, вкладка «Настройки».</div>' : ''}
      <div class="radio-list">${list.map((i, n) => `<label class="${bad(i) ? 'dis' : ''}"><input type="radio" name="inst" value="${esc(i.key)}" ${bad(i) ? 'disabled' : n === firstOk ? 'checked' : ''}><div style="flex:1"><b>${esc(i.name)}</b><div class="hint">Minecraft ${esc(i.mc_version)}, ${esc(i.loader_label)}${bad(i) ? ' — нужен загрузчик' : ''}</div></div></label>`).join('')}</div>`,
    foot: `<button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" id="ch-ok" ${firstOk < 0 ? 'disabled' : ''}>Установить</button>`,
    onMount: m => {
      $('#ch-ok', m).onclick = () => {
        const r = $('input[name=inst]:checked', m);
        if (!r) { toast('Выбери сборку, в которую установить', 'error'); return; }
        closeModal(); installTo(r.value, h, type);
      };
    },
  });
}

/* ====================== скриншоты ====================== */
function shotURL(s) { return `api/shot?key=${enc(s.key)}&file=${enc(s.file)}`; }
function fmtDateTime(ts) { const d = new Date(ts * 1000); return d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', year: 'numeric' }) + ', ' + d.toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' }); }
function shotsHTML() {
  return `<div class="page"><div class="page-head"><div><h1>Скриншоты</h1><p class="sub" id="shots-sub">Все скриншоты из всех сборок</p></div>
    <div class="head-actions"><select id="shots-filter" aria-label="Сборка"><option value="">Все сборки</option></select><button class="btn" data-act="shots-reload">${ic('refresh')} Обновить</button></div></div><div id="shots-box"></div></div>`;
}
async function loadShots() {
  const my = S.shots = { list: null, filter: S.shots?.filter || '', err: '' };
  try { my.list = (await api('shots.list')).shots; } catch (e) { my.err = e.message; }
  drawShots();
}
function shotsVisible() { const L = S.shots.list || []; return S.shots.filter ? L.filter(s => s.key === S.shots.filter) : L; }
function drawShots() {
  const box = $('#shots-box'), T = S.shots;
  if (!box || !T) return;
  const sel = $('#shots-filter');
  if (T.list) {
    const names = new Map(); T.list.forEach(s => names.set(s.key, s.inst));
    sel.innerHTML = '<option value="">Все сборки</option>' + [...names].map(([k, n]) => `<option value="${esc(k)}" ${k === T.filter ? 'selected' : ''}>${esc(n)}</option>`).join('');
    if (T.filter && !names.has(T.filter)) T.filter = '';
  }
  if (T.err) { box.innerHTML = `<div class="empty"><h3>Не получилось загрузить скриншоты</h3><p>${esc(T.err)}</p></div>`; return; }
  if (!T.list) { box.innerHTML = '<div class="empty"><span class="spin"></span></div>'; return; }
  const list = shotsVisible();
  $('#shots-sub').textContent = T.list.length ? `${list.length} ${plural(list.length, 'скриншот', 'скриншота', 'скриншотов')}` : 'Все скриншоты из всех сборок';
  if (!list.length) { box.innerHTML = `<div class="empty"><h3>Пока нет скриншотов</h3><p>Нажми F2 в игре, и снимок появится здесь с названием сборки.</p></div>`; return; }
  box.innerHTML = `<div class="shots-grid">${list.map((s, i) => `<button class="shot" data-act="shot-open" data-i="${i}" aria-label="Открыть скриншот"><img src="${shotURL(s)}" alt="" loading="lazy"><span class="cap"><b>${esc(s.inst)}</b><small>${esc(fmtDateTime(s.mtime))}</small></span></button>`).join('')}</div>`;
}
function shotViewer(i) {
  const list = shotsVisible(); if (!list[i]) return;
  const s = list[i];
  const m = modal({
    title: s.inst, wide: true,
    body: `<div class="viewer"><button class="btn sq nav" data-act="shot-step" data-d="-1" aria-label="Предыдущий" ${i === 0 ? 'disabled' : ''}>${ic('back')}</button>
      <img src="${shotURL(s)}" alt=""><button class="btn sq nav fwd" data-act="shot-step" data-d="1" aria-label="Следующий" ${i === list.length - 1 ? 'disabled' : ''}>${ic('back')}</button></div>
      <p class="hint" style="margin-top:12px">${esc(s.file)} · ${esc(fmtDateTime(s.mtime))} · ${esc(fmtSize(s.size))}</p>`,
    foot: `<button class="btn danger" data-act="shot-del">${ic('trash', 16)}Удалить</button><span style="flex:1"></span><button class="btn" data-act="shot-copy">${ic('copy', 16)} Копировать</button>
      <button class="btn" data-act="shot-save">${ic('download', 16)} В «Изображения»</button><button class="btn" data-act="open-sub" data-key="${esc(s.key)}" data-sub="screenshots">${ic('folder', 16)} Папка</button>`,
  });
  S.shotIdx = i;
  return m;
}

/* ====================== скины ====================== */
// рисуем скин спереди: голова, тело, руки, ноги и второй слой
function drawSkin(canvas, img, slim, k) {
  const w = 16 * k, h = 32 * k; canvas.width = w; canvas.height = h;
  const c = canvas.getContext('2d'); c.imageSmoothingEnabled = false; c.clearRect(0, 0, w, h);
  const modern = img.naturalHeight >= 64, aw = slim ? 3 : 4;
  const blit = (sx, sy, sw, sh, dx, dy, flip) => {
    if (flip) { c.save(); c.translate((dx + sw) * k, dy * k); c.scale(-1, 1); c.drawImage(img, sx, sy, sw, sh, 0, 0, sw * k, sh * k); c.restore(); }
    else c.drawImage(img, sx, sy, sw, sh, dx * k, dy * k, sw * k, sh * k);
  };
  blit(8, 8, 8, 8, 4, 0); blit(20, 20, 8, 12, 4, 8);
  blit(44, 20, aw, 12, 4 - aw, 8); blit(4, 20, 4, 12, 4, 20);
  if (modern) { blit(36, 52, aw, 12, 12, 8); blit(20, 52, 4, 12, 8, 20); } else { blit(44, 20, aw, 12, 12, 8, true); blit(4, 20, 4, 12, 8, 20, true); }
  blit(40, 8, 8, 8, 4, 0);
  if (modern) { blit(20, 36, 8, 12, 4, 8); blit(44, 36, aw, 12, 4 - aw, 8); blit(52, 52, aw, 12, 12, 8); blit(4, 36, 4, 12, 4, 20); blit(4, 52, 4, 12, 8, 20); }
}
function hydrateSkins(root = document) {
  $$('canvas[data-skin]', root).forEach(cv => {
    if (cv.dataset.done === cv.dataset.skin) return;
    cv.dataset.done = cv.dataset.skin;
    const img = new Image();
    img.onload = () => drawSkin(cv, img, cv.dataset.variant === 'slim', +cv.dataset.k || 6);
    img.onerror = () => { cv.classList.add('bad'); };
    img.src = cv.dataset.skin;
  });
}
function skinAcc() {
  const ms = S.st.accounts.filter(a => a.type === 'ms');
  return ms.find(a => a.uuid === S.sk?.uuid) || ms.find(a => a.uuid === S.st.selected) || ms[0] || null;
}
function skinsHTML() {
  return `<div class="page"><div class="page-head"><div><h1>Скины</h1><p class="sub">Смена скина прямо из лаунчера</p></div></div>
    <div class="empty soon">${ic('shirt', 42)}<h3>Скоро</h3><p>Раздел со скинами ещё готовится. Когда он появится, это будет в обновлении лаунчера.</p></div></div>`;
}
async function loadSkins(full = true) {
  const prev = S.sk || {};
  const acc = skinAcc();
  S.sk = { uuid: acc?.uuid, list: prev.list || null, profile: full ? null : prev.profile, err: '', loading: !!acc && full, busy: false };
  drawSkins();
  const jobs = [api('skins.list').then(d => { S.sk.list = d.skins; })];
  if (acc && full) jobs.push(api('skins.profile', { uuid: acc.uuid }).then(p => { S.sk.profile = p; }).catch(e => { S.sk.err = e.message; }));
  await Promise.all(jobs.map(j => j.catch(e => { S.sk.err = S.sk.err || e.message; })));
  S.sk.loading = false; drawSkins();
}
function skinCard(s) {
  return `<article class="skin"><div class="skin-view"><canvas data-skin="api/skinimg?id=${esc(s.id)}" data-variant="${s.variant}" data-k="5" aria-label="Скин ${esc(s.name)}"></canvas></div>
    <div class="skin-info"><b title="${esc(s.name)}">${esc(s.name)}</b><span class="chip">${s.variant === 'slim' ? 'Тонкие руки' : 'Обычные руки'}</span></div>
    <div class="skin-btns"><button class="btn primary sm" data-act="skin-apply" data-id="${esc(s.id)}" ${S.sk.busy ? 'disabled' : ''}>Надеть</button><button class="btn sm sq" data-act="skin-edit" data-id="${esc(s.id)}" title="Изменить">${ic('sliders', 15)}</button></div></article>`;
}
function drawSkins() {
  const box = $('#skins-box'), K = S.sk;
  if (!box || !K) return;
  const accs = S.st.accounts, ms = accs.filter(a => a.type === 'ms'), acc = skinAcc();
  let top;
  if (!ms.length) {
    const ely = accs.some(a => a.type === 'ely');
    top = `<div class="panel noacc"><h2 class="sec">Нужен аккаунт Microsoft</h2><p>Скин лицензионного аккаунта хранится у Mojang, поэтому его можно менять только после входа через Microsoft.${ely ? ' Скин аккаунта Ely.by меняется на сайте ely.by.' : ''}</p>
      <div style="display:flex;gap:10px;margin-top:14px;flex-wrap:wrap"><button class="btn primary" data-act="ms-login">${ic('plus')} Войти через Microsoft</button>${ely ? `<button class="btn" data-act="open-url" data-url="https://ely.by/skins">${ic('link', 15)} Скины Ely.by</button>` : ''}</div></div>`;
  } else {
    const P = K.profile;
    const capes = P?.capes?.length ? `<div class="capes"><span class="hint">Плащ</span><div class="seg"><button data-act="cape-set" data-id="" class="${P.capes.some(c => c.active) ? '' : 'on'}">Без плаща</button>${P.capes.map(c => `<button data-act="cape-set" data-id="${esc(c.id)}" class="${c.active ? 'on' : ''}">${esc(c.alias)}</button>`).join('')}</div></div>` : '';
    top = `<div class="panel current"><div class="cur-view">${P ? `<canvas data-skin="api/skinurl?uuid=${enc(acc.uuid)}&t=${P.skin_url ? enc(P.skin_url.slice(-12)) : 'x'}" data-variant="${P.variant}" data-k="9" aria-label="Текущий скин"></canvas>` : (K.loading ? '<span class="spin"></span>' : '')}</div>
      <div class="cur-info"><div style="display:flex;gap:10px;align-items:center;flex-wrap:wrap"><h2>${esc(acc.name)}</h2>${ms.length > 1 ? `<select id="skin-acc" aria-label="Аккаунт">${ms.map(a => `<option value="${esc(a.uuid)}" ${a.uuid === acc.uuid ? 'selected' : ''}>${esc(a.name)}</option>`).join('')}</select>` : ''}</div>
        <p class="hint">${K.loading ? 'Загружаю профиль…' : P ? (P.skin_url ? `Сейчас надет скин с ${P.variant === 'slim' ? 'тонкими' : 'обычными'} руками` : 'Сейчас надет стандартный скин') : ''}</p>
        ${K.err ? `<div class="note warn" style="margin-top:12px">${esc(K.err)}</div>` : ''}${capes}
        ${P ? `<div class="cur-btns"><button class="btn" data-act="skin-save-current" ${P.skin_url ? '' : 'disabled'}>${ic('download', 16)} Сохранить в библиотеку</button><button class="btn" data-act="skin-reset" ${P.skin_url ? '' : 'disabled'}>Сбросить на стандартный</button></div>` : K.err ? `<div class="cur-btns"><button class="btn" data-act="skins-reload">${ic('refresh', 16)} Повторить</button></div>` : ''}</div></div>`;
  }
  const lib = K.list ? (K.list.length ? `<div class="skin-grid">${K.list.map(skinCard).join('')}</div>`
    : `<div class="empty" style="padding:34px 10px"><h3>Библиотека пуста</h3><p>Загрузи свой .png (64×64), возьми скин любого игрока по нику или сохрани текущий.</p></div>`) : '<div class="empty"><span class="spin"></span></div>';
  box.innerHTML = `${top}<h2 class="sec" style="margin:26px 0 12px">Моя библиотека</h2>${lib}${ms.length ? '' : '<p class="hint" style="margin-top:12px">Библиотеку можно пополнять и без входа: скины сохранятся и пригодятся позже.</p>'}`;
  hydrateSkins(box);
}
function skinEdit(id) {
  const s = (S.sk.list || []).find(x => x.id === id); if (!s) return;
  modal({
    title: 'Скин',
    body: `<div style="display:flex;gap:20px;align-items:flex-start"><div class="skin-view big"><canvas data-skin="api/skinimg?id=${esc(s.id)}" data-variant="${s.variant}" data-k="8"></canvas></div><div style="flex:1">
      <label class="fld"><span>Название</span><input id="sk-name" value="${esc(s.name)}" maxlength="40"></label>
      <div class="fld"><span>Руки</span><div class="seg" id="sk-var"><button data-v="classic" class="${s.variant === 'classic' ? 'on' : ''}">Обычные (Steve)</button><button data-v="slim" class="${s.variant === 'slim' ? 'on' : ''}">Тонкие (Alex)</button></div>
      <small>Если у скина руки выглядят обрезанными или с чёрной полосой, переключи этот вариант.</small></div></div></div>`,
    foot: `<button class="btn danger" id="sk-del">${ic('trash', 16)}Удалить</button><span style="flex:1"></span><button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" id="sk-ok">Сохранить</button>`,
    onMount: m => {
      hydrateSkins(m);
      let v = s.variant;
      $$('#sk-var button', m).forEach(b => b.onclick = () => { v = b.dataset.v; $$('#sk-var button', m).forEach(x => x.classList.toggle('on', x === b)); const cv = $('canvas', m); cv.dataset.variant = v; cv.dataset.done = ''; hydrateSkins(m); });
      $('#sk-ok', m).onclick = async () => { try { await api('skins.update', { id, name: $('#sk-name', m).value, variant: v }); closeModal(); await loadSkins(false); } catch (e) { toast(e.message, 'error'); } };
      $('#sk-del', m).onclick = async () => { try { await api('skins.delete', { id }); closeModal(); toast('Скин удалён из библиотеки', 'ok'); await loadSkins(false); } catch (e) { toast(e.message, 'error'); } };
    },
  });
}
function skinLookup() {
  modal({
    title: 'Скин игрока', body: `<label class="fld"><span>Ник игрока Minecraft</span><input id="sk-nick" placeholder="Например, Notch" maxlength="16" autocomplete="off"><small>Скин добавится в твою библиотеку, потом его можно надеть.</small></label>`,
    foot: `<button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" id="sk-go">Найти</button>`,
    onMount: m => {
      const go = async () => { const b = $('#sk-go', m); b.disabled = true; try { await api('skins.lookup', { nick: $('#sk-nick', m).value }); closeModal(); toast('Скин добавлен в библиотеку', 'ok'); await loadSkins(false); } catch (e) { toast(e.message, 'error'); b.disabled = false; } };
      $('#sk-go', m).onclick = go; $('#sk-nick', m).onkeydown = e => { if (e.key === 'Enter') go(); };
    },
  });
}
async function skinRun(fn, okText) {
  const K = S.sk; if (K.busy) return; K.busy = true; K.err = ''; drawSkins();
  try { const p = await fn(); if (p) { K.profile = p; K.uuid = skinAcc()?.uuid; } toast(okText, 'ok'); }
  catch (e) { K.err = e.message; toast(e.message, 'error'); }
  K.busy = false; drawSkins();
}

/* ====================== настройки приложения ====================== */
/* ====================== тема оформления ====================== */
const THEMES = {
  slate: { name: 'Сланец', accent: '#9b7cff', play: '#ffcf5a', bg: '#0f1117' },
  ocean: { name: 'Океан', accent: '#38bdf8', play: '#fbbf24', bg: '#0b141c' },
  forest: { name: 'Лес', accent: '#4ade80', play: '#f59e0b', bg: '#0e1511' },
  sunset: { name: 'Закат', accent: '#fb7185', play: '#fdba74', bg: '#1a1014' },
  sakura: { name: 'Сакура', accent: '#f472b6', play: '#fde047', bg: '#171018' },
  graphite: { name: 'Графит', accent: '#d4d4d8', play: '#facc15', bg: '#121212' },
  light: { name: 'Светлая', accent: '#6d4aff', play: '#f59e0b', bg: '#f4f5fa', text: '#1b1e2b' },
};
const EVENTS = {
  newyear: { name: 'Новый год', emoji: '🎄', from: [12, 15], to: [1, 10], desc: 'Падает снег, на кнопках сугробы, сверху мигает гирлянда' },
  halloween: { name: 'Хэллоуин', emoji: '🎃', from: [10, 20], to: [11, 2], desc: 'Паутина в углах, паук на нити, летучие мыши и тыква у кнопки «Играть»' },
  valentine: { name: 'День влюблённых', emoji: '💝', from: [2, 10], to: [2, 15], desc: 'Летят сердечки, на кнопках сердечки' },
};
const inRange = (e, d) => { const v = (d.getMonth() + 1) * 100 + d.getDate(), a = e.from[0] * 100 + e.from[1], b = e.to[0] * 100 + e.to[1]; return a <= b ? v >= a && v <= b : v >= a || v <= b; };
function activeEvents(t, d = new Date()) {
  const e = t && t.event;
  if (EVENTS[e]) return [e];
  if (e === 'auto') { const f = Object.entries(EVENTS).find(([, v]) => inRange(v, d)); return f ? [f[0]] : []; }
  return [];
}
const THEME_VARS = ['--bg', '--rail', '--s1', '--s2', '--s3', '--line', '--line-soft', '--line-hi', '--text', '--muted', '--faint', '--accent', '--accent-2', '--accent-bg', '--accent-ink', '--play', '--play-2', '--play-ink'];
const hex2rgb = h => { h = String(h).replace('#', ''); return [0, 2, 4].map(i => parseInt(h.slice(i, i + 2), 16) || 0); };
const rgb2hex = c => '#' + c.map(x => Math.max(0, Math.min(255, Math.round(x))).toString(16).padStart(2, '0')).join('');
const mixc = (a, b, t) => { const x = hex2rgb(a), y = hex2rgb(b); return rgb2hex(x.map((v, i) => v + (y[i] - v) * t)); };
const lumOf = h => { const [r, g, b] = hex2rgb(h).map(v => { v /= 255; return v <= .03928 ? v / 12.92 : ((v + .055) / 1.055) ** 2.4; }); return .2126 * r + .7152 * g + .0722 * b; };
function themeBase(t) {
 return { ...THEMES.slate, ...(t && THEMES[t.preset] ? THEMES[t.preset] : {}), ...(t || {}) }; }
function applyTheme(t) {
  applyFx(t);
  const root = document.documentElement;
  THEME_VARS.forEach(k => root.style.removeProperty(k));
  root.style.colorScheme = '';
  if (!t || !Object.keys(t).length) return;
  const c = themeBase(t), light = lumOf(c.bg) > .45, cc = light ? '#000000' : '#ffffff';
  const text = c.text || (light ? '#1b1e2b' : '#e9eaf2');
  const set = (k, v) => root.style.setProperty(k, v);
  set('--bg', c.bg);
  set('--rail', mixc(c.bg, '#000000', light ? .06 : .35));
  set('--s1', mixc(c.bg, cc, light ? .03 : .04));
  set('--s2', mixc(c.bg, cc, light ? .06 : .08));
  set('--s3', mixc(c.bg, cc, light ? .1 : .15));
  set('--line', mixc(c.bg, cc, light ? .14 : .17));
  set('--line-soft', mixc(c.bg, cc, light ? .08 : .11));
  set('--line-hi', mixc(c.bg, cc, light ? .22 : .26));
  set('--text', text);
  set('--muted', mixc(text, c.bg, .42));
  set('--faint', mixc(text, c.bg, .62));
  set('--accent', c.accent);
  set('--accent-2', mixc(c.accent, light ? '#000000' : '#ffffff', light ? .25 : .35));
  const [r, g, b] = hex2rgb(c.accent);
  set('--accent-bg', `rgba(${r}, ${g}, ${b}, .14)`);
  set('--accent-ink', lumOf(c.accent) > .4 ? '#10131c' : '#ffffff');
  set('--play', c.play);
  set('--play-2', mixc(c.play, '#000000', .12));
  set('--play-ink', lumOf(c.play) > .4 ? '#2b1d00' : '#ffffff');
  root.style.colorScheme = light ? 'light' : 'dark';
}
/* ====================== праздничные украшения ====================== */
function applyFx(t) {
  const root = document.documentElement, ids = activeEvents(t);
  [...root.classList].filter(c => c.startsWith('fx-')).forEach(c => root.classList.remove(c));
  ids.forEach(id => root.classList.add('fx-' + id));
  let fx = document.getElementById('fx');
  if (!ids.length) { if (fx) fx.remove(); return; }
  if (!fx) { fx = document.createElement('div'); fx.id = 'fx'; fx.setAttribute('aria-hidden', 'true'); document.body.appendChild(fx); }
  const r = (a, b) => a + Math.random() * (b - a);
  const web = '<svg viewBox="0 0 120 120"><g stroke="#cfd3e6" stroke-width="1" fill="none"><path d="M0 0H120M0 0L110 45M0 0L85 85M0 0L45 110M0 0V120"/><path d="M22 0Q20 20 0 22M46 0Q42 42 0 46M72 0Q64 64 0 72M100 0Q88 88 0 100"/></g></svg>';
  let h = '';
  if (ids.includes('newyear')) {
    const cols = ['#ff5a5a', '#ffd34d', '#5ad17a', '#5ab0ff'], n = Math.ceil(innerWidth / 44);
    h += '<div class="garland">' + Array.from({ length: n }, (_, i) => `<i style="left:${(i + .5) * 100 / n}%;--c:${cols[i % 4]};--d:${(i % 5) * .35}s;top:${6 + Math.sin(i * Math.PI / n * 6) * 3 + 3}px"></i>`).join('') + '</div>';
    h += Array.from({ length: 36 }, (_, i) => `<b class="flake" style="left:${r(0, 100)}%;--s:${r(3, 8)}px;--t:${r(9, 20)}s;--w:${r(-40, 40)}px;animation-delay:${-r(0, 20)}s;opacity:${r(.5, .95)}"></b>`).join('');
  }
  if (ids.includes('halloween')) {
    const web2 = '<svg viewBox="0 0 140 140"><g stroke="#e7e9f7" stroke-width="1.3" fill="none" stroke-linecap="round"><path d="M0 0L140 0M0 0L132 40M0 0L108 90M0 0L70 124M0 0L30 138M0 0L0 140"/><path d="M26 0Q24 24 0 26M52 0Q46 46 0 52M80 0Q70 70 0 80M110 0Q96 96 0 110"/></g></svg>';
    const pump = '<svg viewBox="0 0 100 92"><path d="M46 22Q47 6 62 3L60 13Q55 15 55 24Z" fill="#4d7c0f"/><ellipse cx="50" cy="56" rx="46" ry="34" fill="#ea580c"/><ellipse cx="27" cy="56" rx="25" ry="33" fill="#f97316"/><ellipse cx="73" cy="56" rx="25" ry="33" fill="#f97316"/><ellipse cx="50" cy="56" rx="17" ry="34" fill="#fb923c"/><path d="M26 44L41 44L33 58Z M59 44L74 44L67 58Z M50 58L44 67L56 67Z" fill="#fef08a"/><path d="M27 70Q50 90 73 70L67 69L62 76L56 69L50 77L44 69L38 76L33 69Z" fill="#fef08a"/></svg>';
    const bat = '<svg viewBox="0 0 60 30"><path d="M30 11C27 2 13 0 0 7C8 8 10 14 14 19C18 14 22 16 26 23L30 18L34 23C38 16 42 14 46 19C50 14 52 8 60 7C47 0 33 2 30 11Z" fill="#241538" stroke="#8b5cf6" stroke-width="1.2"/></svg>';
    const spider = '<svg viewBox="0 0 40 36"><g stroke="#8b5cf6" stroke-width="1.6" fill="none" stroke-linecap="round"><path d="M14 16L4 8M14 18L2 18M14 20L4 30M26 16L36 8M26 18L38 18M26 20L36 30"/></g><circle cx="20" cy="22" r="9" fill="#241538" stroke="#8b5cf6" stroke-width="1.2"/><circle cx="20" cy="12" r="5" fill="#241538" stroke="#8b5cf6" stroke-width="1.2"/><circle cx="18" cy="11" r="1.3" fill="#f87171"/><circle cx="22" cy="11" r="1.3" fill="#f87171"/></svg>';
    h += '<div class="mist"></div>';
    h += `<div class="web tl">${web2}</div><div class="web tr">${web2}</div>`;
    h += `<div class="spider"><i></i>${spider}</div>`;
    h += `<span class="bat" style="--y:14%;--t:21s;animation-delay:-4s">${bat}</span><span class="bat sm" style="--y:34%;--t:30s;animation-delay:-17s">${bat}</span>`;
    h += `<span class="pumpkin p1">${pump}</span><span class="pumpkin p2">${pump}</span>`;
  }
  if (ids.includes('valentine')) {
    h += Array.from({ length: 22 }, () => `<b class="heart" style="left:${r(0, 100)}%;--s:${r(12, 26)}px;--t:${r(8, 16)}s;--w:${r(-30, 30)}px;animation-delay:${-r(0, 16)}s">${Math.random() < .5 ? '❤' : '💗'}</b>`).join('');
  }
  fx.innerHTML = h;
}
const fxKeep = () => { const t = S.theme || {}; return t.event ? { event: t.event } : {}; };
const saveThemeSoon = debounce(() => {
  api('settings.set', { theme: S.theme || {} }).then(d => { S.st.settings = d.settings; }).catch(e => toast(e.message, 'error'));
}, 400);
function setTheme(t, save = true) { S.theme = t; applyTheme(t); if (save) saveThemeSoon(); }

/* ====================== настройки ====================== */
const SET_TABS = [['game', 'Игра', 'sliders'], ['accounts', 'Аккаунты и сервисы', 'globe'], ['look', 'Внешний вид', 'shirt'], ['data', 'Данные', 'folder'], ['about', 'Обновления', 'refresh']];
function settingsHTML() {
  const s = S.st.settings, tab = S.setTab || 'game';
  const f = (id, label, val, hint = '', type = 'text', ph = '') => `<label class="fld"><span>${label}</span><input type="${type}" id="${id}" value="${esc(val)}" placeholder="${esc(ph)}">${hint ? `<small>${hint}</small>` : ''}</label>`;
  let body = '';
  if (tab === 'game') {
    body = `<section class="panel"><h2 class="sec">Игра</h2>${f('st-ram', 'Память по умолчанию, МБ', s.ram_mb, 'Можно переопределить в настройках каждой сборки.', 'number')}
      ${f('st-java', 'Путь к Java', s.java_path, 'Оставь пустым — лаунчер сам скачает нужную Java.', 'text', 'Автоматически')}</section>`;
  } else if (tab === 'accounts') {
    body = `<section class="panel"><h2 class="sec">CurseForge</h2>${f('st-cf', 'Свой ключ API CurseForge', s.cf_api_key || '', 'Необязательно. Если ключ уже встроен в лаунчер, оставь поле пустым. Свой бесплатный ключ: console.curseforge.com → API keys.', 'text')}</section>`;
  } else if (tab === 'look') {
    const t = S.theme || {}, c = themeBase(t);
    const row = (key, label, hint) => `<div class="color-row"><div><b>${label}</b><small>${hint}</small></div><input type="color" id="th-${key}" value="${esc(c[key] || (key === 'text' ? (lumOf(c.bg) > .45 ? '#1b1e2b' : '#e9eaf2') : '#000000'))}" aria-label="${label}"></div>`;
    const tile = ([id, p]) => `<button class="preset ${t.preset === id && !t.accent && !t.play && !t.bg && !t.text ? 'on' : ''}" data-act="theme-preset" data-id="${id}"><div class="pv" style="background:${p.bg}"><i style="background:${p.accent}"></i><i style="background:${p.play}"></i></div><span>${p.emoji ? p.emoji + ' ' : ''}${esc(p.name)}</span></button>`;
    const evs = activeEvents(t);
    body = `<section class="panel"><h2 class="sec">Готовые темы</h2><div class="presets">${Object.entries(THEMES).map(tile).join('')}</div></section>
      <section class="panel"><h2 class="sec">Праздничные украшения</h2>
        <div class="colors">${[['', '🚫', 'Без украшений', 'Обычный вид'], ['auto', '📅', 'По датам', evs.length ? 'Сейчас идёт: ' + EVENTS[evs[0]].emoji + ' ' + EVENTS[evs[0]].name : 'Включаются сами в нужные дни: Новый год, Хэллоуин, 14 февраля'], ...Object.entries(EVENTS).map(([id, e]) => [id, e.emoji, e.name, e.desc])].map(([id, em, name, desc]) => `<label class="color-row" style="cursor:pointer"><div><b>${em} ${esc(name)}</b><small>${esc(desc)}</small></div><input type="radio" name="th-event" id="th-event-${id || 'off'}" ${(t.event || '') === id ? 'checked' : ''}></label>`).join('')}</div>
        <p class="hint" style="margin-top:10px">Одновременно работает только одно событие. «По датам»: Новый год с 15 декабря по 10 января, Хэллоуин с 20 октября по 2 ноября, 14 февраля с 10 по 15 февраля.</p></section>
      <section class="panel"><h2 class="sec">Свои цвета</h2><div class="colors">
        ${row('accent', 'Основной цвет', 'Кнопки, выделение, значки')}
        ${row('play', 'Кнопка «Играть»', 'Цвет главной кнопки')}
        ${row('bg', 'Фон', 'Остальные оттенки подбираются сами')}
        ${row('text', 'Текст', 'Цвет надписей')}</div>
        <div style="margin-top:16px"><button class="btn sm" data-act="theme-reset">Сбросить на стандартные</button></div>
        <p class="hint" style="margin-top:10px">Изменения применяются сразу и запоминаются.</p></section>`;
  } else if (tab === 'about') {
    const u = S.upd || {};
    body = `<section class="panel support-panel"><h2 class="sec">Поддержать проект</h2><p class="hint">Voxel делается в свободное время. Если лаунчер пригодился, можно поддержать его на Boosty.</p>
      <div style="margin-top:14px"><button class="btn primary" data-act="open-url" data-url="https://boosty.to/kingo0offf">${ic('heart', 15)} Поддержать на Boosty</button></div></section>
      <section class="panel"><h2 class="sec">Версия</h2><p>Установлена: <b>${esc(S.st.version)}</b></p>
      <p class="hint" style="margin-top:6px">${u.error ? esc(u.error) : u.available ? 'Доступна новая версия: <b>' + esc(u.latest) + '</b>' : u.checked ? 'У тебя последняя версия.' : 'Проверка ещё не выполнялась.'}</p>
      ${u.available && u.notes ? `<p class="hint" style="margin-top:8px;white-space:pre-wrap">${esc(u.notes)}</p>` : ''}
      <div style="display:flex;gap:10px;flex-wrap:wrap;margin-top:14px">
        <button class="btn" data-act="update-check" ${u.busy ? 'disabled' : ''}>${ic('refresh', 15)} Проверить обновления</button>
        ${u.available ? `<button class="btn primary" data-act="update-install" ${u.busy ? 'disabled' : ''}>${u.busy === 'install' ? 'Скачиваю…' : 'Обновить и перезапустить'}</button>` : ''}</div></section>`;
  } else {
    body = `<section class="panel"><h2 class="sec">Данные</h2><p class="hint" style="margin-bottom:12px;overflow-wrap:anywhere">${esc(S.st.data_dir)}</p>
      <div style="display:flex;gap:10px;flex-wrap:wrap"><button class="btn sm" data-act="open-app" data-what="data">${ic('folder', 15)} Папка данных</button><button class="btn sm" data-act="open-app" data-what="exports">${ic('folder', 15)} Папка экспорта</button></div></section>`;
  }
  const saves = tab === 'game' || tab === 'accounts';
  return `<div class="page"><div class="page-head"><div><h1>Настройки</h1><p class="sub">Версия ${esc(S.st.version)}</p></div><div class="head-actions">${saves ? '<button class="btn primary" data-act="save-app-settings">Сохранить</button>' : ''}</div></div>
    <div class="set-wrap"><nav class="set-nav" aria-label="Разделы настроек">${SET_TABS.map(([id, label, icon]) => `<button class="${tab === id ? 'on' : ''}" data-act="set-tab" data-tab="${id}">${ic(icon, 18)}${label}${id === 'about' && S.upd && S.upd.available ? '<i class="tab-dot" title="Есть обновление"></i>' : ''}</button>`).join('')}</nav>
    <div class="set-body">${body}</div></div></div>`;
}

/* ====================== сеть: комнаты ====================== */
function networkHTML() {
  return `<div class="page"><div class="page-head"><div><h1>Сеть</h1><p class="sub">Играйте вместе по сети без настройки роутера и без VPN</p></div></div><div id="net-box" class="form-narrow" style="display:grid;gap:18px"><span class="spin"></span></div></div>`;
}
async function loadNetwork(st) {
  const box = $('#net-box'); if (!box) return;
  let d;
  try { d = st || await api('room.status'); } catch (e) { box.innerHTML = `<p class="err-text">${esc(e.message)}</p>`; return; }
  if (!$('#net-box')) return;
  // Страница обновляется само, но только если что-то изменилось и пока ты не печатаешь.
  const sig = JSON.stringify([d.mode, d.status, d.peers, d.world_port, d.refused, d.error, d.listen_port, d.manual_port]);
  const typing = document.activeElement && ['net-port', 'net-code'].includes(document.activeElement.id);
  if (st === undefined && (sig === S.netSig || typing)) { clearTimeout(S.netTimer); if (d.mode) S.netTimer = setTimeout(() => { if (route().name === 'network') loadNetwork(); }, 2500); return; }
  S.netSig = sig;
  const how = `<p class="hint">Игра идёт напрямую между компьютерами (шифруется), а если не получается, через публичный сервер-посредник. Работает только для Minecraft. Все должны быть на одной и той же версии игры.</p>`;
  if (d.mode === 'host') {
    const wp = d.world_port;
    box.innerHTML = `<section class="panel"><h2 class="sec">Твоя комната открыта</h2>
      <p class="hint" style="margin-bottom:10px">Отправь код другу. Он вставит его на вкладке «Сеть» и нажмёт «Подключиться».</p>
      <div class="code-box" style="font-size:15px;overflow-wrap:anywhere;user-select:all">${esc(d.code)}</div>
      <div style="display:flex;gap:10px;flex-wrap:wrap;margin-top:12px"><button class="btn primary" data-act="copy-text" data-text="${esc(d.code)}">${ic('copy', 15)} Скопировать код</button><button class="btn" data-act="net-leave">Закрыть комнату</button></div>
      <p style="margin-top:14px">Друзей подключено: <b>${d.peers}</b></p></section>
      <section class="panel"><h2 class="sec">Мир для друзей</h2>
      ${wp ? `<p>Найден открытый мир на порту <b>${wp}</b>. Друзья попадут в него.</p>` : `<p class="hint">Запусти игру, открой свой мир и в меню паузы нажми «Открыть для сети». Лаунчер сам найдёт порт. Если игра запущена не из лаунчера, впиши порт вручную.</p>`}
      <div style="display:flex;gap:10px;flex-wrap:wrap;margin-top:12px;align-items:center"><input type="number" id="net-port" min="1024" max="65535" placeholder="Порт вручную" value="${d.manual_port || ''}" style="max-width:180px"><button class="btn sm" data-act="net-port">Применить</button></div>
      ${d.refused ? `<p class="err-text" style="margin-top:10px">Попыток войти без открытого мира: ${d.refused}. Открой мир для сети.</p>` : ''}</section>${how}`;
  } else if (d.mode === 'guest') {
    const lost = d.status === 'lost';
    box.innerHTML = `<section class="panel"><h2 class="sec">${lost ? 'Связь потеряна' : 'Ты в комнате'}</h2>
      ${lost ? `<p class="err-text">${esc(d.error)}</p>` : `<p>Открой Minecraft: в «Сетевой игре» появится мир <b>«Комната друзей»</b>. Нажми на него и играй.</p>
      <p class="hint" style="margin-top:10px">Если мир не появился, нажми «Прямое подключение» и введи адрес <b>localhost${d.listen_port === 25565 ? '' : ':' + d.listen_port}</b>.</p>`}
      <div style="display:flex;gap:10px;flex-wrap:wrap;margin-top:14px"><button class="btn" data-act="net-leave">Выйти из комнаты</button></div></section>${how}`;
  } else {
    box.innerHTML = `<section class="panel"><h2 class="sec">Создать комнату</h2><p class="hint" style="margin-bottom:12px">Ты хост: друзья зайдут в твой мир. Нужен интернет.</p><button class="btn primary" data-act="net-host">${ic('plus', 16)} Создать комнату</button></section>
      <section class="panel"><h2 class="sec">Подключиться к другу</h2><label class="fld"><span>Код комнаты</span><input type="text" id="net-code" placeholder="Вставь код от друга" autocomplete="off" spellcheck="false"></label><button class="btn primary" data-act="net-join">Подключиться</button></section>${how}`;
  }
  clearTimeout(S.netTimer);
  if (d.mode) S.netTimer = setTimeout(() => { if (route().name === 'network') loadNetwork(); }, 2500);
}
async function netRun(fn) {
  try { await loadNetwork(await fn()); } catch (e) { toast(e.message, 'error'); S.netSig = ''; loadNetwork(); }
}

/* ====================== аккаунты ====================== */
function acctMenu(anchor) {
  const accs = S.st.accounts, sel = curAccount();
  const pop = popAt(anchor, `${accs.map(a => `<div class="acct ${sel && a.uuid === sel.uuid ? 'sel' : ''}">${avatar(a, 'avatar-sm')}
      <button class="who" data-act="acct-select" data-uuid="${esc(a.uuid)}"><b>${esc(a.name)}</b><small>${a.type === 'ely' ? 'Ely.by' : 'Microsoft'}</small></button>
      ${sel && a.uuid === sel.uuid ? `<span class="tick">${ic('check', 16)}</span>` : ''}
      <button class="btn ghost sq sm" data-act="acct-remove" data-uuid="${esc(a.uuid)}" data-type="${a.type}" title="Убрать аккаунт">${ic('x', 15)}</button></div>`).join('')}
    ${accs.length ? '<hr>' : ''}<button class="item" data-act="ely-login">${ic('plus', 16)} Добавить Ely.by</button>`, { up: true });
  hydrateAvatars(pop);
}
function elyLogin() {
  closePop();
  modal({
    title: 'Вход через Ely.by',
    body: `<label class="fld"><span>Ник или e-mail</span><input type="text" id="ely-u" autocomplete="username"></label>
      <label class="fld"><span>Пароль</span><input type="password" id="ely-p" autocomplete="current-password"></label>
      <label class="fld"><span>Код двухфакторной защиты</span><input type="text" id="ely-t" inputmode="numeric" placeholder="Только если она включена"></label><p class="err-text" id="m-err"></p>`,
    foot: '<button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" id="ely-go">Войти</button>',
    onMount: m => {
      const run = async () => {
        const b = $('#ely-go', m), err = $('#m-err', m);
        err.textContent = ''; b.disabled = true;
        try {
          const d = await api('accounts.ely_login', { username: $('#ely-u', m).value, password: $('#ely-p', m).value, totp: $('#ely-t', m).value });
          await refresh(); closeModal(); render({ soft: true }); toast(`Добро пожаловать, ${d.name}!`, 'ok');
        } catch (e) {
          err.textContent = e.message + (/two.?factor|2fa/i.test(e.message) ? ' Введи код из приложения-аутентификатора в нижнее поле.' : '');
          b.disabled = false;
        }
      };
      $('#ely-go', m).onclick = run;
      m.onkeydown = e => { if (e.key === 'Enter') run(); };
    },
  });
}
function msLogin(codeMode = false) {
  closePop();
  if (codeMode !== true) codeMode = false;
  modal({
    title: 'Вход через Microsoft', body: '<div id="ms-body" style="text-align:center;padding:20px 0"><span class="spin"></span><p style="margin-top:10px">Связываюсь с Microsoft…</p></div>',
    foot: '<button class="btn" data-act="ms-cancel">Отмена</button>',
  });
  api('accounts.ms_start', { mode: codeMode ? 'code' : 'browser' }).then(d => {
    const body = $('#ms-body');
    if (!body) return;
    if (d.state === 'error') { body.innerHTML = `<p class="err-text">${esc(d.error)}</p>`; return; }
    if (codeMode) {
      body.innerHTML = `<p>Открой страницу и введи этот код. Войди в аккаунт, на котором куплен Minecraft.</p><div class="code-box">${esc(d.code)}</div>
        <div style="display:flex;gap:10px;justify-content:center"><button class="btn primary" data-act="open-url" data-url="${esc(d.url)}">${ic('link', 16)} Открыть страницу</button><button class="btn" data-act="copy-text" data-text="${esc(d.code)}">${ic('copy', 16)} Скопировать код</button></div>
        <p class="hint" style="margin-top:14px" id="ms-state">Жду подтверждения…</p>`;
    } else {
      body.innerHTML = `<p>Открылась страница Microsoft. Войди там в аккаунт, на котором куплен Minecraft. Пароль вводится на сайте Microsoft, лаунчер его не видит.</p>
        <div style="display:flex;gap:10px;justify-content:center;margin-top:14px;flex-wrap:wrap"><button class="btn primary" data-act="open-url" data-url="${esc(d.url)}">${ic('link', 16)} Открыть страницу снова</button><button class="btn" data-act="ms-code">Войти по коду</button></div>
        <p class="hint" style="margin-top:14px" id="ms-state">Жду, пока ты войдёшь в браузере…</p>`;
    }
    api('app.open_url', { url: d.url }).catch(() => { });
    S.msPoll = setInterval(async () => {
      try {
        const s = await api('accounts.ms_status');
        if (s.state === 'done') { await refresh(); closeModal(); render({ soft: true }); toast('Аккаунт Microsoft добавлен', 'ok'); }
        else if (s.state === 'error') { clearInterval(S.msPoll); S.msPoll = null; const el = $('#ms-state'); if (el) { el.className = 'err-text'; el.textContent = s.error; } }
      } catch (e) { /* повторим */ }
    }, 2000);
  }).catch(e => { const b = $('#ms-body'); if (b) b.innerHTML = `<p class="err-text">${esc(e.message)}</p>`; });
}

/* ====================== экспорт ====================== */
function openExport(key) {
  modal({
    title: 'Экспорт в .zip',
    body: `<p class="hint" style="margin-bottom:12px">Архив можно отправить другу — он импортирует его в своём лаунчере.</p><label class="chk" style="margin:0"><input type="checkbox" id="ex-w"> Включить миры (архив станет больше)</label><p class="err-text" id="m-err"></p>`,
    foot: '<button class="btn" data-act="modal-close">Отмена</button><button class="btn primary" id="ex-go">Экспортировать</button>',
    onMount: m => {
      $('#ex-go', m).onclick = async () => {
        const b = $('#ex-go', m); b.disabled = true; b.innerHTML = '<span class="spin"></span> Упаковываю…';
        try {
          const r = await api('instances.export', { key, include_worlds: $('#ex-w', m).checked });
          closeModal(); toast(`Экспортировано: ${r.path}`, 'ok'); api('app.open', { what: 'exports' }).catch(() => { });
        } catch (e) { $('#m-err', m).textContent = e.message; b.disabled = false; b.textContent = 'Экспортировать'; }
      };
    },
  });
}

/* ====================== действия ====================== */
const A = {
  nav: el => { closePop(); go(el.dataset.to); },
  open: el => goInst(el.dataset.key),
  tab: el => { const r = route(); goInst(r.key, el.dataset.tab); },
  'modal-close': () => closeModal(),
  'modal-bg': (el, e) => { if (e.target === el) closeModal(); },
  create: () => { closePop(); openCreate('new'); },
  import: () => openCreate('import'),
  'imp-pick': () => { const f = $('#imp-file'); f.onchange = () => importFile(f.files[0]); f.click(); },
  'create-go': async el => {
    const name = $('#cr-name').value.trim();
    el.disabled = true;
    try {
      const v = S.vp.get();
      const d = await api('instances.create', { name, ...v, ram_mb: $('#cr-ram').value, java_args: $('#cr-jvm').value });
      await refresh(); closeModal(); toast(`Сборка «${d.instance.name}» создана`, 'ok'); goInst(d.instance.key);
    } catch (e) { toast(e.message, 'error'); el.disabled = false; }
  },
  play: async el => { try { await api('play', { key: el.dataset.key }); S.tasks[el.dataset.key] = { phase: 'preparing' }; if (S.wake) S.wake(); await refresh(); render({ soft: true }); } catch (e) { toast(e.message, 'error'); if (/аккаунт/i.test(e.message)) acctMenu($('.acct-btn')); } },
  stop: async el => { await api('stop', { key: el.dataset.key }); },
  'lib-filter': el => { S.lib.filter = el.dataset.v; $$('#lib-filter button').forEach(b => b.classList.toggle('on', b === el)); $('#grid').innerHTML = gridHTML(); },
  'inst-folder': el => api('instances.open_folder', { key: el.dataset.key }),
  'open-sub': el => api('instances.open_folder', { key: el.dataset.key, sub: el.dataset.sub }),
  'open-app': el => api('app.open', { what: el.dataset.what }),
  'open-url': el => api('app.open_url', { url: el.dataset.url }),
  'copy-text': async el => { try { await navigator.clipboard.writeText(el.dataset.text); toast('Скопировано', 'ok'); } catch (e) { toast('Не удалось скопировать — выдели текст вручную', 'error'); } },
  'inst-menu': el => {
    const k = esc(el.dataset.key);
    popAt(el, `<button class="item" data-act="inst-export" data-key="${k}">${ic('upload', 16)} Экспорт в .zip</button>
      <button class="item" data-act="inst-dup" data-key="${k}">${ic('copy', 16)} Сделать копию</button><hr><button class="item danger" data-act="inst-delete" data-key="${k}">${ic('trash', 16)} Удалить сборку</button>`, { right: true });
  },
  'inst-export': el => { closePop(); openExport(el.dataset.key); },
  'inst-dup': async el => {
    closePop();
    const it = getItem(el.dataset.key);
    const name = await askText({ title: 'Копия сборки', label: 'Название копии', value: it.name + ' (копия)', ok: 'Создать копию' });
    if (!name) return;
    toast('Копирую…');
    const d = await api('instances.duplicate', { key: it.key, name });
    await refresh(); toast('Копия создана', 'ok'); goInst(d.instance.key);
  },
  'inst-delete': async el => {
    closePop();
    const it = getItem(el.dataset.key);
    if (!await confirmBox({ title: 'Удалить сборку?', text: `«${esc(it.name)}» будет удалена вместе с мирами, модами и настройками. Это нельзя отменить.`, ok: 'Удалить', danger: true })) return;
    await api('instances.delete', { id: it.id }); await refresh(); toast('Сборка удалена', 'ok'); go('library');
  },
  'icon-toggle': () => { S.ic.open = !S.ic.open; renderIconPicker(); },
  'icon-seed': el => { S.ic.icon = ''; S.ic.icon_seed = el.dataset.s; S.ic.preview = ''; renderIconPicker(); },
  'icon-upload': () => {
    const f = $('#icon-file');
    f.onchange = async () => {
      const file = f.files[0]; f.value = ''; if (!file) return;
      try {
        const { blob, data } = await squareIcon(file);
        const d = await upload('icon', { key: S.ic.key }, new File([blob], 'icon.png', { type: 'image/png' }));
        S.ic.icon = 'custom'; S.ic.preview = data; S.ic.icon_v = d.instance ? d.instance.icon_v : 0;
        await refresh(); renderIconPicker(); toast('Своя картинка загружена', 'ok');
      } catch (e) { toast(e.message, 'error'); }
    };
    f.click();
  },
  'ram-preset': el => { $('#s-ram').value = el.dataset.v; },
  'save-settings': async el => {
    el.disabled = true;
    try {
      const v = S.vp.get();
      await api('instances.update', { id: el.dataset.id, name: $('#s-name').value, ...v, ram_mb: $('#s-ram').value || 0, java_args: $('#s-jvm').value, notes: $('#s-notes').value, icon: S.ic.icon, icon_seed: S.ic.icon_seed });
      await refresh(); toast('Сохранено', 'ok'); render({ soft: true, hard: true });
    } catch (e) { toast(e.message, 'error'); } finally { el.disabled = false; }
  },
  'net-host': el => { el.disabled = true; toast('Создаю комнату…'); return netRun(() => api('room.host', {})); },
  'net-join': el => { const code = $('#net-code').value.trim(); if (!code) { toast('Вставь код комнаты', 'error'); return; } el.disabled = true; toast('Подключаюсь…'); return netRun(() => api('room.join', { code })); },
  'net-leave': () => netRun(() => api('room.leave')),
  'net-port': () => netRun(() => api('room.set_port', { port: +$('#net-port').value || 0 })),
  'save-app-settings': async () => {
    const body = {};
    for (const [k, id, num] of [['ram_mb', 'st-ram', true], ['java_path', 'st-java'], ['cf_api_key', 'st-cf']]) {
      const el = $('#' + id); if (el) body[k] = num ? +el.value : el.value;
    }
    const d = await api('settings.set', body);
    S.st.settings = d.settings; toast('Настройки сохранены', 'ok');
    setTimeout(async () => { await refresh(); }, 1500);
  },
  'update-check': async () => { await checkUpdate(true); render(); },
  'update-install': async () => {
    S.upd = { ...S.upd, busy: 'install' }; render();
    try { await api('app.update_install', {}); toast('Перезапускаю лаунчер…', 'ok'); }
    catch (e) { S.upd = { ...S.upd, busy: false }; render(); toast(e.message, 'error'); }
  },
  'set-tab': el => { S.setTab = el.dataset.tab; render(); },
  'theme-preset': el => { setTheme({ ...fxKeep(), preset: el.dataset.id }); render(); },
  'theme-reset': () => { setTheme(fxKeep()); render(); toast('Цвета сброшены', 'ok'); },
  'acct-menu': (el, e) => { e.stopPropagation(); acctMenu($('.acct-btn')); },
  'acct-select': async el => { await api('accounts.select', { uuid: el.dataset.uuid }); await refresh(); closePop(); render({ soft: true }); },
  'acct-remove': async el => { await api('accounts.remove', { uuid: el.dataset.uuid, type: el.dataset.type }); await refresh(); closePop(); render({ soft: true }); },
  'ely-login': elyLogin,
  'ms-login': () => msLogin(false),
  'ms-code': () => msLogin(true),
  'ms-cancel': () => { api('accounts.ms_cancel').catch(() => { }); closeModal(); },
  /* моды */
  'mods-add': () => { const f = $('#file-mod'); f.onchange = () => uploadFiles([...f.files], 'mods'); f.click(); },
  'mod-toggle': async el => { try { await api('mods.toggle', { key: S.mods.key, file: el.dataset.file }); S.mods = { ...(await api('mods.list', { key: S.mods.key })), key: S.mods.key, local: true }; } catch (e) { toast(e.message, 'error'); } drawMods(); },
  'mod-delete': async el => {
    if (!await confirmBox({ title: 'Удалить мод?', text: esc(el.dataset.file), ok: 'Удалить', danger: true })) return;
    await api('mods.delete', { key: S.mods.key, files: [el.dataset.file] });
    S.mods = { ...(await api('mods.list', { key: S.mods.key })), key: S.mods.key, local: true }; drawMods(); refresh();
  },
  'mods-check': async el => {
    el.disabled = true; el.innerHTML = '<span class="spin"></span> Проверяю…';
    try {
      const d = await api('mods.check_updates', { key: S.mods.key });
      S.mods.updates = d.updates.map(u => u.file);
      toast(d.updates.length ? `Найдено обновлений: ${d.updates.length}` : 'Все моды актуальны', d.updates.length ? 'info' : 'ok');
    } catch (e) { toast(e.message, 'error'); }
    el.disabled = false; el.innerHTML = `${ic('refresh')} Проверить обновления`; drawMods();
  },
  'mod-update': async el => { await modUpdate([el.dataset.file]); },
  'mods-update-all': async () => { await modUpdate(S.mods.updates); },
  /* миры */
  'world-backup': async el => { const d = await api('worlds.backup', { key: el.dataset.key, name: el.dataset.name }); toast(`Копия сохранена: ${d.path}`, 'ok'); },
  'world-delete': async el => {
    if (!await confirmBox({ title: 'Удалить мир?', text: `Мир «${esc(el.dataset.name)}» будет удалён навсегда. Сначала можно сделать резервную копию.`, ok: 'Удалить', danger: true })) return;
    await api('worlds.delete', { key: el.dataset.key, name: el.dataset.name }); toast('Мир удалён', 'ok'); loadTab(getItem(el.dataset.key), 'worlds');
  },
  'log-copy': async () => { try { await navigator.clipboard.writeText($('#logbox').innerText); toast('Журнал скопирован', 'ok'); } catch (e) { toast('Не удалось скопировать', 'error'); } },
  /* Modrinth */
  'br-type': el => { S.br.type = el.dataset.t; S.br.tags.clear(); drawTags(); $$('#br-types button').forEach(b => b.classList.toggle('on', b === el)); brSearch(true); },
  'br-tag': el => { const B = S.br, t = el.dataset.t; B.tags.has(t) ? B.tags.delete(t) : B.tags.add(t); drawTags(); brSearch(true); },
  'lim-toggle': () => drawLimit(!$('#br-lim .vsel-menu')),
  'lim-set': el => { const B = S.br, n = +el.dataset.n; drawLimit(false); if (!B || !LIMITS.includes(n) || n === B.limit) return; B.limit = n; lsSet('br-limit', n); drawLimit(); brSearch(true); },
  'br-tags-clear': () => { S.br.tags.clear(); drawTags(); brSearch(true); },
  'br-src': el => { const B = S.br; if (B.src === el.dataset.s) return; B.src = el.dataset.s; B.tags.clear(); drawTags(); $$('#br-src button').forEach(b => b.classList.toggle('on', b === el)); $('#br-q').placeholder = 'Искать на ' + (B.src === 'curseforge' ? 'CurseForge' : 'Modrinth'); brSearch(true); },
  /* скриншоты и скины */
  'shots-reload': () => loadShots(),
  'shot-open': el => shotViewer(+el.dataset.i),
  'shot-step': el => shotViewer(S.shotIdx + +el.dataset.d),
  'shot-del': async () => {
    const s = shotsVisible()[S.shotIdx]; if (!s) return;
    if (!await confirmBox({ title: 'Удалить скриншот?', text: esc(s.inst + ' · ' + s.file), ok: 'Удалить', danger: true })) return;
    await api('shots.delete', { key: s.key, file: s.file }); S.shots.list = S.shots.list.filter(x => !(x.key === s.key && x.file === s.file)); drawShots(); toast('Скриншот удалён', 'ok');
  },
  'shot-copy': async () => {
    const s = shotsVisible()[S.shotIdx]; if (!s) return;
    try { const blob = await (await fetch(shotURL(s))).blob(); await navigator.clipboard.write([new ClipboardItem({ 'image/png': blob.type === 'image/png' ? blob : await new Promise(r => { const i = new Image(); i.onload = () => { const c = document.createElement('canvas'); c.width = i.width; c.height = i.height; c.getContext('2d').drawImage(i, 0, 0); c.toBlob(r, 'image/png'); }; i.src = URL.createObjectURL(blob); }) })]); toast('Скриншот скопирован', 'ok'); }
    catch (e) { toast('Не удалось скопировать. Используй «В «Изображения»».', 'error'); }
  },
  'shot-save': async () => { const s = shotsVisible()[S.shotIdx]; if (!s) return; const d = await api('shots.save_copy', { key: s.key, file: s.file }); toast('Сохранено: ' + d.path, 'ok'); },
  'skins-reload': () => loadSkins(),
  'skin-upload': () => { const f = $('#skin-file'); f.onchange = async () => { const file = f.files[0]; f.value = ''; if (!file) return; try { await upload('skin', {}, file); toast('Скин добавлен в библиотеку', 'ok'); await loadSkins(false); } catch (e) { toast(e.message, 'error'); } }; f.click(); },
  'skin-lookup': skinLookup,
  'skin-edit': el => skinEdit(el.dataset.id),
  'skin-apply': el => {
    const s = S.sk.list.find(x => x.id === el.dataset.id), acc = skinAcc();
    if (!acc) { toast('Сначала войди через Microsoft', 'error'); return; }
    return skinRun(() => api('skins.apply', { uuid: acc.uuid, id: s.id }), `Скин «${s.name}» надет. В игре он обновится при следующем входе`);
  },
  'skin-reset': async () => { const acc = skinAcc(); if (acc && await confirmBox({ title: 'Сбросить скин?', text: 'Вернётся стандартный скин Minecraft. Твой текущий скин пропадёт, если не сохранить его в библиотеку.', ok: 'Сбросить', danger: true })) skinRun(() => api('skins.reset', { uuid: acc.uuid }), 'Скин сброшен'); },
  'cape-set': el => { const acc = skinAcc(); return skinRun(() => api('skins.cape', { uuid: acc.uuid, cape_id: el.dataset.id }), el.dataset.id ? 'Плащ надет' : 'Плащ снят'); },
  'skin-save-current': async () => { const acc = skinAcc(); await api('skins.import_current', { uuid: acc.uuid }); toast('Скин сохранён в библиотеку', 'ok'); await loadSkins(false); },
  'br-page': el => { const B = S.br, n = +el.dataset.n, max = Math.ceil(B.total / brLimit(B)); if (!(n >= 0 && n < max) || n === B.page) return; B.page = n; brSearch(false); $('#br-q')?.scrollIntoView?.({ block: 'start' }); },
  'br-open': el => { const h = S.br.hits.find(x => x.project_id === el.dataset.pid); if (h) openProject(h); },
  'br-back': () => { const D = S.br.detail; if (D && D.pushed && history.state && history.state.brDetail) history.back(); else { S.br.detail = null; drawDetail(); } },
  'br-tab': el => { const D = S.br.detail; if (!D) return; D.tab = el.dataset.t; drawDetail(); if (D.tab === 'versions') loadVersions(); },
  'br-img': el => {
    const o = document.createElement('div'); o.className = 'lightbox'; o.setAttribute('data-act', 'br-img-close'); o.innerHTML = `<img src="${esc(el.dataset.url)}" alt="">`;
    document.body.appendChild(o);
  },
  'br-img-close': el => el.remove(),
  'open-link': el => api('app.open_link', { url: el.dataset.url }).catch(e => toast(e.message, 'error')),
  'br-retry': () => brSearch(true),
  'br-install': async el => {
    const B = S.br, h = B.hits.find(x => x.project_id === el.dataset.pid);
    if (!h) return;
    if (B.type === 'modpack') {
      B.busy.add(h.project_id); drawHits(); toast(`Создаю сборку «${h.title}»…`);
      try { const d = await api(B.src + '.install', { project_id: h.project_id, type: 'modpack', title: h.title, icon_url: h.icon_url, site_url: h.site_url || '' }); await refresh(); toast(`Сборка «${d.instance.name}» создана`, 'ok'); if (d.skipped?.length) toast(`${d.skipped.length} ${plural(d.skipped.length, 'мод не скачан', 'мода не скачано', 'модов не скачано')}: авторы запретили загрузку из лаунчеров. Скачай их на сайте CurseForge и добавь во вкладке «Моды».`, 'error'); goInst(d.instance.key); }
      catch (e) { toast(e.message, 'error'); } finally { B.busy.delete(h.project_id); drawHits(); }
      return;
    }
    if (!B.ctx) return chooseInstance(h, B.type);
    installTo(B.ctx, h, B.type);
  },
};
async function modUpdate(files) {
  try {
    const d = await api('mods.update', { key: S.mods.key, files });
    toast(`Обновлено модов: ${d.updated}`, 'ok');
    S.mods = { ...(await api('mods.list', { key: S.mods.key })), key: S.mods.key, local: true };
    S.mods.updates = [];
  } catch (e) { toast(e.message, 'error'); }
  drawMods();
}
async function uploadFiles(files, folder) {
  const r = route(); let ok = 0;
  for (const f of files) {
    try { await upload('content', { key: r.key, folder }, f); ok++; } catch (e) { toast(`${f.name}: ${e.message}`, 'error'); }
  }
  if (ok) {
    toast(`Добавлено файлов: ${ok}`, 'ok');
    await refresh();
    if (r.tab === 'mods') loadTab(getItem(r.key), 'mods');
  }
}

/* ====================== события ====================== */
document.addEventListener('click', e => {
  const el = e.target.closest('[data-act]');
  if (!e.target.closest('#br-lim') && $('#br-lim .vsel-menu')) drawLimit(false);
  if (!el) { if (!e.target.closest('.pop')) closePop(); return; }
  const fn = A[el.dataset.act];
  if (!fn) return;
  if (el.tagName === 'A') e.preventDefault();
  if (el.dataset.act !== 'acct-menu' && el.dataset.act !== 'inst-menu' && !e.target.closest('.pop')) closePop();
  Promise.resolve(fn(el, e)).catch(err => toast(err.message || String(err), 'error'));
});
document.addEventListener('keydown', e => {
  if (e.key === 'Escape') { if ($('#modal-root .backdrop')) closeModal(); else closePop(); return; }
  if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('[data-act][role=link]')) { e.preventDefault(); e.target.click(); }
});
document.addEventListener('input', e => {
  const id = e.target.id;
  if (id === 'lib-q') { S.lib.q = e.target.value; $('#grid').innerHTML = gridHTML(); $('#hero').innerHTML = heroHTML(); }
  else if (id === 'mod-q') { S.modq = e.target.value; drawMods(); }
  else if (id === 'br-q') debouncedBr(e.target.value);
  else if (id && id.startsWith('th-event-')) { const v = id.slice(9), t = { ...(S.theme || {}) }; if (v === 'off') delete t.event; else t.event = v; setTheme(t); }
  else if (id && id.startsWith('th-')) { const k = id.slice(3); setTheme({ ...(S.theme || {}), [k]: e.target.value }); }
});
const debouncedBr = debounce(v => { if (S.br) { S.br.q = v.trim(); brSearch(true); } }, 350);
document.addEventListener('change', e => {
  const id = e.target.id;
  if (id === 'lib-sort') { S.lib.sort = e.target.value; $('#grid').innerHTML = gridHTML(); }
  else if (id === 'shots-filter') { S.shots.filter = e.target.value; drawShots(); }
  else if (id === 'skin-acc') { S.sk.uuid = e.target.value; loadSkins(true); }
  else if (S.br && id === 'br-sort') { S.br.sort = e.target.value; brSearch(true); }
  else if (S.br && id === 'br-mc') { S.br.mc = e.target.value; brSearch(true); }
  else if (S.br && id === 'br-ld') { S.br.loader = e.target.value; brSearch(true); }
});

/* перетаскивание файлов */
let dragDepth = 0;
const hasFiles = e => e.dataTransfer && [...e.dataTransfer.types].includes('Files');
function dropKind() {
  const r = route();
  if (r.name === 'i' && getItem(r.key)?.kind === 'local' && r.tab === 'mods') return { kind: 'mods', text: 'Отпусти .jar-файлы, чтобы добавить моды' };
  if (r.name === 'library') return { kind: 'import', text: 'Отпусти .mrpack или .zip, чтобы импортировать сборку' };
  return null;
}
addEventListener('dragenter', e => {
  if (!hasFiles(e)) return;
  dragDepth++;
  const k = dropKind();
  if (k) { $('.drag-hint')?.remove(); document.body.insertAdjacentHTML('beforeend', `<div class="drag-hint">${esc(k.text)}</div>`); document.body.classList.add('dragging'); }
});
addEventListener('dragleave', e => { if (hasFiles(e) && --dragDepth <= 0) { dragDepth = 0; document.body.classList.remove('dragging'); $('.drag-hint')?.remove(); } });
addEventListener('dragover', e => { if (hasFiles(e)) e.preventDefault(); });
addEventListener('drop', e => {
  if (!hasFiles(e)) return;
  e.preventDefault();
  dragDepth = 0; document.body.classList.remove('dragging'); $('.drag-hint')?.remove();
  const files = [...e.dataTransfer.files], k = dropKind();
  if (!k) return;
  if (k.kind === 'mods') uploadFiles(files.filter(f => f.name.toLowerCase().endsWith('.jar')), 'mods');
  else importFile(files.find(f => /\.(mrpack|zip)$/i.test(f.name)));
});

/* ====================== отрисовка и опрос ====================== */
function render(opts = {}) {
  const r = route();
  if (!S.st) return;
  const ae = document.activeElement, fid = ae && ae.id, pos = ae && ae.selectionStart;
  if (opts.soft && !opts.hard && r.name === 'i' && $('#inst-actions')) {
    renderRail(r);
    const it = getItem(r.key);
    if (it) {
      $('#inst-actions').innerHTML = instActions(it);
      if (['overview', 'log'].includes(r.tab)) loadTab(it, r.tab);
    }
    return;
  }
  stopLog();
  if (!opts.soft) closePop();
  renderRail(r);
  const v = $('#view');
  if (r.name === 'library') v.innerHTML = libraryHTML();
  else if (r.name === 'discover') { v.innerHTML = discoverHTML(); browseInto($('#browse'), null); }
  else if (r.name === 'shots') { v.innerHTML = shotsHTML(); if (S.shots?.list && opts.soft) drawShots(); else loadShots(); }
  else if (r.name === 'skins') { v.innerHTML = skinsHTML(); }
  else if (r.name === 'network') { v.innerHTML = networkHTML(); S.netSig = ''; loadNetwork(); }
  else if (r.name === 'settings') v.innerHTML = settingsHTML();
  else if (r.name === 'i') renderInstance(r);
  else go('library');
  if (!opts.soft) $('#main').scrollTop = 0;
  if (fid) { const n = document.getElementById(fid); if (n && n !== document.activeElement) { n.focus(); try { n.setSelectionRange(pos, pos); } catch (e) { /* не текстовое поле */ } } }
}
addEventListener('hashchange', () => render());
/* «Назад» на мыши или в системе закрывает страницу мода, а не уходит на главную */
addEventListener('popstate', () => { const B = S.br; if (B && B.detail) { B.detail = null; drawDetail(); } });

function updateProgressUI() {
  const r = route(), entries = Object.entries(S.prog);
  const preparing = Object.values(S.tasks).some(t => t.phase === 'preparing');
  const e = entries.find(([k]) => k === r.key) || entries.find(([k]) => k === 'global') || entries[0];
  const bar = $('#gprog'), pill = $('#statuspill');
  if (!e && !preparing) { bar.hidden = true; pill.hidden = true; return; }
  const v = e ? e[1] : { label: 'Готовлю запуск', done: 0, total: 0 };
  bar.hidden = false;
  bar.classList.toggle('indeterminate', !v.total);
  $('i', bar).style.width = v.total ? (v.done / v.total * 100) + '%' : '';
  pill.hidden = false;
  pill.innerHTML = `<span class="spin"></span>${esc(v.label)}${v.total ? ` ${v.done} из ${v.total}` : '…'}`;
}
const phases = t => JSON.stringify(Object.fromEntries(Object.entries(t).map(([k, v]) => [k, v.phase])));
async function checkUpdate(manual) {
  S.upd = { ...(S.upd || {}), busy: manual ? 'check' : false };
  try {
    const d = await api('app.update_check', {});
    S.upd = { ...d, checked: true, busy: false };
    if (d.available && !manual) toast(`Вышла новая версия ${d.latest}. Установить: Настройки → Обновления`, 'info');
    if (manual) toast(d.available ? `Найдена версия ${d.latest}` : 'У тебя последняя версия', d.available ? 'info' : 'ok');
  } catch (e) { S.upd = { ...S.upd, busy: false, error: manual ? e.message : '' }; if (manual) toast(e.message, 'error'); }
  if (!manual) { try { renderRail(route()); } catch (e) { } }
}
async function pollLoop() {
  for (;;) {
    try {
      const p = await apiGet('poll', { notice: S.nid });
      S.nid = p.notice;
      p.notices.forEach(n => toast(n.text, n.kind));
      const before = phases(S.tasks);
      S.tasks = p.tasks; S.prog = p.progress;
      updateProgressUI();
      if (before !== phases(S.tasks)) { await refresh(); render({ soft: true }); }
    } catch (e) { /* офлайн показывает request() */ }
    // ждём до следующего опроса, но «проснуться» можно раньше (S.wake) — например, сразу после нажатия «Играть»
    await new Promise(r => { S.wake = r; setTimeout(r, Object.values(S.tasks).some(t => t.phase === 'preparing') ? 500 : 1500); });
  }
}

const SPLASH_FROM = Date.now();
function hideSplash() {
  const s = document.getElementById('splash');
  if (!s) return;
  setTimeout(() => { s.classList.add('out'); setTimeout(() => s.remove(), 450); }, Math.max(0, 1300 - (Date.now() - SPLASH_FROM)));
}
(async function init() {
  try { await refresh(); } catch (e) {
    $('#view').innerHTML = `<div class="page"><div class="empty"><h3>Не удалось загрузить данные</h3><p>${esc(e.message)}</p></div></div>`;
    hideSplash();
    return;
  }
  render();
  hideSplash();
  pollLoop();
  checkUpdate(false);
  addEventListener('pagehide', () => navigator.sendBeacon('api/bye', '{}'));
})();
