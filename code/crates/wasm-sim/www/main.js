import init, { WasmSim } from './pkg/arc_malachitebft_wasm_sim.js';

const STEP_COLORS = {
  Unstarted: '#9aa0a6',
  Propose: '#4c8bf5',
  Prevote: '#e3a008',
  Precommit: '#a142f4',
  Commit: '#0f9d58',
};

const MSG_COLORS = {
  Proposal: '#4c8bf5',
  Prevote: '#e3a008',
  Precommit: '#a142f4',
  PolkaCertificate: '#e8710a',
  RoundCertificate: '#d93025',
  SyncRequest: '#12b5cb',
  SyncResponse: '#0b8a9b',
};

const LEGEND_LABELS = {
  Proposal: 'Proposal',
  Prevote: 'Prevote',
  Precommit: 'Precommit',
  PolkaCertificate: 'Polka cert',
  RoundCertificate: 'Round cert',
  SyncRequest: 'Sync req',
  SyncResponse: 'Sync resp',
};

// Identity colors, used to show who proposed each decided value.
const NODE_COLORS = [
  '#4c8bf5', '#0f9d58', '#e8710a', '#a142f4', '#d93025', '#12b5cb', '#c5221f', '#7cb342',
  '#f06292', '#5c6bc0', '#8d6e63', '#26a69a', '#ab47bc', '#ffa000', '#546e7a', '#9e9d24',
];

const $ = (id) => document.getElementById(id);
const canvas = $('net');
const ctx = canvas.getContext('2d');

let sim = null;
let statuses = [];
let inflight = [];   // MessageSent events still being animated
let effects = [];    // Short-lived visual effects: { node, kind, label, until }
let proposerOf = new Map(); // node -> proposer of its current round
let decidedHeights = new Map(); // height -> { value, round, time }, first decision seen
let counters = { sent: 0, dropped: 0 };
let playing = true;
let simTime = 0;
let lastFrame = performance.now();
let chainKey = '';
const shown = new Set(Object.keys(MSG_COLORS));

// ----- Controls -----

const speed = () => 10 ** (($('speed').value - 60) / 30);

function syncOutputs() {
  const s = speed();
  $('speedOut').textContent = (s < 1 ? s.toFixed(2) : s.toFixed(1)) + '×';
  $('dropOut').textContent = $('drop').value + '%';
  $('minDelayOut').textContent = $('minDelay').value + ' ms';
  $('maxDelayOut').textContent = $('maxDelay').value + ' ms';
}

function delays() {
  const a = +$('minDelay').value;
  const b = +$('maxDelay').value;
  return [Math.min(a, b), Math.max(a, b)];
}

const TIMEOUT_FIELDS = [
  'propose', 'propose_delta', 'prevote', 'prevote_delta',
  'precommit', 'precommit_delta', 'rebroadcast', 'max_timeout',
];

function timeouts() {
  const t = {};
  for (const f of TIMEOUT_FIELDS) {
    const el = $('t_' + f);
    t[f] = Math.max(+el.min, Math.round(+el.value || 0));
  }
  return t;
}

const syncGrace = () => Math.max(0, Math.round(+$('syncGrace').value || 0));

function reset() {
  const n = Math.max(1, Math.min(16, +$('nodes').value || 4));
  const seed = Math.max(0, +$('seed').value || 0);
  const [min, max] = delays();

  if (sim) sim.free();
  sim = new WasmSim(JSON.stringify({
    num_nodes: n,
    seed,
    min_delay_ms: min,
    max_delay_ms: max,
    drop_rate: $('drop').value / 100,
    timeouts: timeouts(),
    sync_grace_ms: syncGrace(),
  }));

  simTime = 0;
  inflight = [];
  effects = [];
  proposerOf = new Map();
  decidedHeights = new Map();
  counters = { sent: 0, dropped: 0 };
  chainKey = '';
  $('log').textContent = '';
  statuses = JSON.parse(sim.statuses());
  renderChain();
}

$('play').onclick = () => {
  playing = !playing;
  $('play').textContent = playing ? 'Pause' : 'Play';
};
$('stepBtn').onclick = () => advance(100);
$('reset').onclick = reset;
$('nodes').onchange = reset;
$('seed').onchange = reset;
for (const f of TIMEOUT_FIELDS) {
  $('t_' + f).onchange = () => sim.set_timeouts(JSON.stringify(timeouts()));
}
$('syncGrace').onchange = () => sim.set_sync_grace_ms(syncGrace());
$('speed').oninput = syncOutputs;
$('drop').oninput = () => { syncOutputs(); sim.set_drop_rate($('drop').value / 100); };
for (const id of ['minDelay', 'maxDelay']) {
  $(id).oninput = () => { syncOutputs(); sim.set_delay_ms(...delays()); };
}

for (const [kind, color] of Object.entries(MSG_COLORS)) {
  const label = document.createElement('label');
  label.innerHTML = `<input type="checkbox" checked><span class="sw" style="background:${color}"></span>${LEGEND_LABELS[kind]}`;
  label.querySelector('input').onchange = (e) => {
    if (e.target.checked) shown.add(kind); else shown.delete(kind);
  };
  $('legend').appendChild(label);
}

canvas.addEventListener('click', (e) => {
  const rect = canvas.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;
  const { positions, radius } = layout();
  positions.forEach((p, i) => {
    if (Math.hypot(p.x - x, p.y - y) <= radius + 4) {
      const online = !statuses[i].online;
      sim.set_online(i, online);
      log(sim.now(), `v${i} ${online ? 'back online' : 'taken offline'}`);
      statuses = JSON.parse(sim.statuses());
    }
  });
});

canvas.addEventListener('mousemove', (e) => {
  const rect = canvas.getBoundingClientRect();
  const { positions, radius } = layout();
  const over = positions.some((p) => Math.hypot(p.x - (e.clientX - rect.left), p.y - (e.clientY - rect.top)) <= radius + 4);
  canvas.style.cursor = over ? 'pointer' : 'default';
});

// ----- Simulation -----

function log(time, text) {
  const el = $('log');
  el.textContent = `${(time / 1000).toFixed(2).padStart(7)}s  ${text}\n` + el.textContent;
  if (el.textContent.length > 20000) el.textContent = el.textContent.slice(0, 15000);
}

function handle(ev) {
  const realNow = performance.now();
  switch (ev.type) {
    case 'MessageSent':
      counters.sent++;
      if (ev.dropped) counters.dropped++;
      inflight.push(ev);
      if (ev.kind === 'SyncRequest') log(ev.time, `v${ev.from} is behind, requests height ${ev.height} from v${ev.to}`);
      break;
    case 'RoundStarted':
      proposerOf.set(ev.node, ev.proposer);
      if (ev.round > 0 && ev.node === ev.proposer) {
        log(ev.time, `height ${ev.height} moves to round ${ev.round}, v${ev.proposer} proposes`);
      }
      break;
    case 'TimeoutElapsed':
      if (ev.kind !== 'Rebroadcast') {
        effects.push({ node: ev.node, kind: 'timeout', label: `timeout ${ev.kind.toLowerCase()}`, until: realNow + 1200 });
      }
      break;
    case 'Decided':
      effects.push({ node: ev.node, kind: 'decided', until: realNow + 700 });
      if (!decidedHeights.has(ev.height)) {
        decidedHeights.set(ev.height, ev);
        log(ev.time, `height ${ev.height} decided: value ${ev.value} (round ${ev.round}, proposed by v${ev.value % 100})`);
      }
      break;
  }
}

function advance(dtSim) {
  simTime += dtSim;
  sim.run_until(Math.floor(simTime));
  for (const ev of JSON.parse(sim.drain_events())) handle(ev);
  statuses = JSON.parse(sim.statuses());
}

// ----- Rendering -----

function layout() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const n = statuses.length || 1;
  const ring = Math.min(w, h) * 0.36;
  const radius = Math.max(16, Math.min(38, (2 * Math.PI * ring) / n / 3.2));
  const positions = statuses.map((_, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / n;
    return { x: w / 2 + ring * Math.cos(a), y: h / 2 + ring * Math.sin(a) };
  });
  return { positions, radius, w, h };
}

function resizeCanvas() {
  const dpr = window.devicePixelRatio || 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  if (canvas.width !== Math.round(w * dpr) || canvas.height !== Math.round(h * dpr)) {
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
  }
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
}

function draw() {
  resizeCanvas();
  const css = getComputedStyle(document.documentElement);
  const fg = css.getPropertyValue('--fg').trim();
  const muted = css.getPropertyValue('--muted').trim();
  const panel = css.getPropertyValue('--panel').trim();
  const border = css.getPropertyValue('--border').trim();

  const { positions, radius, w, h } = layout();
  ctx.clearRect(0, 0, w, h);

  // Links
  ctx.strokeStyle = border;
  ctx.lineWidth = 1;
  for (let i = 0; i < positions.length; i++) {
    for (let j = i + 1; j < positions.length; j++) {
      ctx.beginPath();
      ctx.moveTo(positions[i].x, positions[i].y);
      ctx.lineTo(positions[j].x, positions[j].y);
      ctx.stroke();
    }
  }

  // Messages
  inflight = inflight.filter((m) => simTime < m.deliver_at);
  for (const m of inflight) {
    if (!shown.has(m.kind)) continue;
    const a = positions[m.from];
    const b = positions[m.to];
    if (!a || !b) continue;
    const span = Math.max(1, m.deliver_at - m.time);
    const p = Math.min(1, Math.max(0, (simTime - m.time) / span));

    // Offset perpendicular to the link so that both directions stay visible.
    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy) || 1;
    const ox = (-dy / len) * 5;
    const oy = (dx / len) * 5;

    // Start at the sender's edge, end at the receiver's edge.
    const start = radius / len;
    const t = start + (1 - 2 * start) * (m.dropped ? Math.min(p, 0.5) : p);
    const x = a.x + dx * t + ox;
    const y = a.y + dy * t + oy;

    if (m.dropped && p > 0.5) {
      const alpha = 1 - (p - 0.5) * 2;
      ctx.globalAlpha = alpha;
      ctx.strokeStyle = '#d93025';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x - 4, y - 4); ctx.lineTo(x + 4, y + 4);
      ctx.moveTo(x + 4, y - 4); ctx.lineTo(x - 4, y + 4);
      ctx.stroke();
      ctx.globalAlpha = 1;
      continue;
    }

    ctx.fillStyle = MSG_COLORS[m.kind];
    ctx.beginPath();
    const r = m.kind === 'Proposal' || m.kind.startsWith('Sync') ? 5.5 : 3.5;
    if (m.kind.endsWith('Certificate')) {
      ctx.rect(x - r, y - r, 2 * r, 2 * r);
    } else {
      ctx.arc(x, y, r, 0, 2 * Math.PI);
    }
    ctx.fill();
    if (m.value === null && (m.kind === 'Prevote' || m.kind === 'Precommit')) {
      // Nil vote: hollow center
      ctx.fillStyle = panel;
      ctx.beginPath();
      ctx.arc(x, y, 1.6, 0, 2 * Math.PI);
      ctx.fill();
    }
  }

  // Nodes
  const realNow = performance.now();
  effects = effects.filter((e) => e.until > realNow);

  statuses.forEach((s, i) => {
    const { x, y } = positions[i];
    const color = s.online ? STEP_COLORS[s.step] || muted : muted;

    for (const e of effects) {
      if (e.node !== i || e.kind !== 'decided') continue;
      const k = 1 - (e.until - realNow) / 700;
      ctx.strokeStyle = STEP_COLORS.Commit;
      ctx.globalAlpha = 1 - k;
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.arc(x, y, radius + 4 + k * 18, 0, 2 * Math.PI);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    ctx.fillStyle = panel;
    ctx.strokeStyle = color;
    ctx.lineWidth = 4;
    ctx.setLineDash(s.online ? [] : [5, 4]);
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, 2 * Math.PI);
    ctx.fill();
    ctx.stroke();
    ctx.setLineDash([]);

    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = s.online ? fg : muted;
    ctx.font = `600 ${Math.round(radius * 0.42)}px system-ui, sans-serif`;
    ctx.fillText(`v${i}`, x, y - radius * 0.18);
    ctx.font = `${Math.round(radius * 0.32)}px ui-monospace, Menlo, monospace`;
    ctx.fillStyle = muted;
    ctx.fillText(`H${s.height} R${Math.max(0, s.round)}`, x, y + radius * 0.3);

    // Step label below the node
    ctx.font = '12px system-ui, sans-serif';
    ctx.fillStyle = color;
    ctx.fillText(s.online ? s.step : 'offline', x, y + radius + 14);

    // Proposer badge
    if (s.online && proposerOf.get(i) === i) {
      ctx.fillStyle = STEP_COLORS.Propose;
      ctx.beginPath();
      ctx.arc(x + radius * 0.78, y - radius * 0.78, 9, 0, 2 * Math.PI);
      ctx.fill();
      ctx.fillStyle = '#fff';
      ctx.font = '600 11px system-ui, sans-serif';
      ctx.fillText('P', x + radius * 0.78, y - radius * 0.78 + 0.5);
    }

    // Timeout labels float above the node
    const timeouts = effects.filter((e) => e.node === i && e.kind === 'timeout');
    timeouts.forEach((e, k) => {
      const age = 1 - (e.until - realNow) / 1200;
      ctx.globalAlpha = 1 - age;
      ctx.fillStyle = '#e8710a';
      ctx.font = '12px system-ui, sans-serif';
      ctx.fillText(e.label, x, y - radius - 12 - k * 14 - age * 10);
      ctx.globalAlpha = 1;
    });
  });
}

function renderChain() {
  const key = statuses.map((s) => s.decisions.length).join(',');
  if (key === chainKey) return;
  chainKey = key;

  const MAX = 14;
  $('chainRows').innerHTML = statuses.map((s, i) => {
    const d = s.decisions;
    const from = Math.max(0, d.length - MAX);
    const blocks = d.slice(from).map((v, k) => {
      const height = from + k + 1;
      const proposer = v % 100;
      return `<span class="block" style="background:${NODE_COLORS[proposer % NODE_COLORS.length]}" title="height ${height}, proposed by v${proposer}">${v}</span>`;
    }).join('');
    const more = from > 0 ? `<span class="block more">… ${from} more</span>` : '';
    return `<div class="chainrow"><span class="who" style="color:${NODE_COLORS[i % NODE_COLORS.length]}">v${i}</span>${more}${blocks}</div>`;
  }).join('');
}

function renderStats() {
  const maxHeight = statuses.reduce((m, s) => Math.max(m, s.decisions.length), 0);
  const dropPct = counters.sent ? ((100 * counters.dropped) / counters.sent).toFixed(1) : '0.0';
  $('stats').textContent =
    `t = ${(simTime / 1000).toFixed(2)} s · ${maxHeight} heights decided · ` +
    `${counters.sent} messages sent, ${dropPct}% dropped`;
}

function frame(now) {
  // Clamp the real elapsed time, so that a backgrounded tab does not jump ahead.
  const dt = Math.min(100, now - lastFrame);
  lastFrame = now;
  if (playing) advance(dt * speed());
  draw();
  renderChain();
  renderStats();
  requestAnimationFrame(frame);
}

await init();
syncOutputs();
reset();
requestAnimationFrame(frame);
