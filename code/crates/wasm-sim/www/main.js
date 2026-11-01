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

// Votes seen by each node: `${node}|${height}|${round}|${kind}` -> Map(voter -> value).
// A node sees its own vote when it sends it, and the others' when they arrive.
let votes = new Map();
let nodeLook = []; // Per-node display state, eased towards the target every frame.
let moments = new Set(); // Keys of the one-off moments already shown, eg. `polka|3|0`.
let chainCounts = []; // Decisions per node at the last chain render.
let hoverHeight = null; // Height of the chain block under the pointer
let hoverNodes = new Set(); // Nodes to highlight on the canvas

function recordVote(node, m) {
  const key = `${node}|${m.height}|${m.round}|${m.kind}`;
  let seen = votes.get(key);
  if (!seen) votes.set(key, (seen = new Map()));
  seen.set(m.from, m.value);

  // The first node to see a polka (a quorum of prevotes for a value) lights up
  // the links between the voters.
  if (m.kind === 'Prevote' && m.value !== null && once(`polka|${m.height}|${m.round}`, false)) {
    const voters = [...seen].filter(([, v]) => v === m.value).map(([voter]) => voter);
    if (voters.length >= quorum(statuses.length)) {
      once(`polka|${m.height}|${m.round}`);
      effects.push({ node: -1, kind: 'polka', voters, until: performance.now() + 900 });
    }
  }
}

// Whether `key` has not been seen yet, marking it as seen unless `mark` is false.
function once(key, mark = true) {
  if (moments.has(key)) return false;
  if (mark) moments.add(key);
  return true;
}

function votesSeen(node, height, round, kind) {
  return votes.get(`${node}|${height}|${round}|${kind}`)?.size ?? 0;
}

// Smallest number of equal-weight votes that is more than two thirds of `n`.
const quorum = (n) => Math.floor((2 * n) / 3) + 1;

const hexToRgb = (hex) => [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16));
const rgb = ([r, g, b]) => `rgb(${r | 0}, ${g | 0}, ${b | 0})`;
const ease = (from, to, k) => from + (to - from) * k;

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
  votes = new Map();
  nodeLook = [];
  moments = new Set();
  chainCounts = [];
  $('banner').hidden = true;
  $('log').replaceChildren();
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

// Hovering a block highlights that height in every chain, and its proposer on the canvas.
$('chainRows').addEventListener('mouseover', (e) => {
  const block = e.target.closest('.block[data-height]');
  if (!block) return;
  hoverHeight = +block.dataset.height;
  hoverNodes = new Set([+block.dataset.proposer]);
  highlightChain();
});
$('chainRows').addEventListener('mouseleave', () => {
  hoverHeight = null;
  hoverNodes = new Set();
  highlightChain();
});

function highlightChain() {
  const rows = $('chainRows');
  rows.classList.toggle('hovering', hoverHeight !== null);
  for (const b of rows.querySelectorAll('.block[data-height]')) {
    b.classList.toggle('hl', +b.dataset.height === hoverHeight);
  }
}

$('clearLog').onclick = () => $('log').replaceChildren();

// Message filters, as toggle chips over the canvas.
for (const [kind, color] of Object.entries(MSG_COLORS)) {
  const chip = document.createElement('button');
  chip.className = 'chip';
  chip.title = `Show or hide ${LEGEND_LABELS[kind].toLowerCase()} messages`;
  chip.setAttribute('aria-pressed', 'true');
  chip.innerHTML = `<span class="sw" style="background:${color}"></span>${LEGEND_LABELS[kind]}`;
  chip.onclick = () => {
    const on = !shown.has(kind);
    if (on) shown.add(kind); else shown.delete(kind);
    chip.classList.toggle('off', !on);
    chip.setAttribute('aria-pressed', String(on));
  };
  $('legend').appendChild(chip);
}

// Legend for the node ring colors.
for (const step of ['Propose', 'Prevote', 'Precommit', 'Commit']) {
  const item = document.createElement('span');
  item.innerHTML = `<i class="ring" style="border-color:${STEP_COLORS[step]}"></i>${step}`;
  $('steps').appendChild(item);
}
{
  const item = document.createElement('span');
  item.textContent = 'Arcs: votes seen, tick = quorum';
  $('steps').prepend(item);
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
      log(sim.now(), `v${i} ${online ? 'back online' : 'taken offline'}`, online ? '' : 'fault');
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

const LOG_LIMIT = 300;

// Newest entries first. `kind` is one of '', 'decided', 'round', 'sync', 'fault'.
function log(time, text, kind = '') {
  const el = $('log');
  const entry = document.createElement('div');
  entry.className = `entry ${kind}`;
  const t = document.createElement('span');
  t.className = 't';
  t.textContent = `${(time / 1000).toFixed(2)}s`;
  const m = document.createElement('span');
  m.className = 'm';
  m.textContent = text;
  entry.append(t, m);
  el.prepend(entry);
  while (el.childElementCount > LOG_LIMIT) el.lastElementChild.remove();
}

function handle(ev) {
  const realNow = performance.now();
  switch (ev.type) {
    case 'MessageSent':
      counters.sent++;
      if (ev.dropped) counters.dropped++;
      inflight.push(ev);
      if (ev.kind === 'Prevote' || ev.kind === 'Precommit') recordVote(ev.from, ev);
      if (ev.kind === 'SyncRequest') {
        log(ev.time, `v${ev.from} is behind, requests height ${ev.height} from v${ev.to}`, 'sync');
        effects.push({ node: -1, kind: 'beam', from: ev.from, to: ev.to, until: realNow + 1800 });
      }
      break;
    case 'RoundStarted':
      proposerOf.set(ev.node, ev.proposer);
      if (ev.round > 0 && ev.node === ev.proposer) {
        log(ev.time, `height ${ev.height} moves to round ${ev.round}, v${ev.proposer} proposes`, 'round');
      }
      if (ev.round > 0 && once(`round|${ev.height}|${ev.round}`)) {
        banner(`Height ${ev.height} · Round ${ev.round}`, `v${ev.proposer} proposes`);
      }
      break;
    case 'TimeoutElapsed':
      if (ev.kind !== 'Rebroadcast') {
        effects.push({ node: ev.node, kind: 'timeout', label: `timeout ${ev.kind.toLowerCase()}`, until: realNow + 1200 });
        effects.push({ node: ev.node, kind: 'shake', until: realNow + 450 });
      }
      break;
    case 'Decided':
      effects.push({ node: ev.node, kind: 'decided', until: realNow + 700 });
      if (!decidedHeights.has(ev.height)) {
        decidedHeights.set(ev.height, ev);
        effects.push({ node: -1, kind: 'burst', until: realNow + 1100 });
        log(ev.time, `height ${ev.height} decided: value ${ev.value} (round ${ev.round}, proposed by v${ev.value % 100})`, 'decided');
      }
      break;
  }
}

function banner(title, subtitle) {
  const el = $('banner');
  el.querySelector('b').textContent = title;
  el.querySelector('span').textContent = subtitle;
  el.hidden = false;
  // Restart the animation.
  el.classList.remove('show');
  void el.offsetWidth;
  el.classList.add('show');
}

function advance(dtSim) {
  simTime += dtSim;
  sim.run_until(Math.floor(simTime));
  for (const ev of JSON.parse(sim.drain_events())) handle(ev);
  statuses = JSON.parse(sim.statuses());

  for (const m of inflight) {
    if (m.arrived || simTime < m.deliver_at) continue;
    m.arrived = true;
    if (!m.dropped && statuses[m.to]?.online) arrive(m);
  }

  // Forget the votes of heights every node has left behind.
  if (votes.size > 4000) {
    const low = Math.min(...statuses.map((s) => s.height));
    for (const key of votes.keys()) {
      if (+key.split('|')[1] < low) votes.delete(key);
    }
  }
}

function arrive(m) {
  if (m.kind === 'Prevote' || m.kind === 'Precommit') recordVote(m.to, m);
  if (shown.has(m.kind)) {
    effects.push({ node: m.to, kind: 'ripple', color: MSG_COLORS[m.kind], until: performance.now() + 380 });
  }
}

// ----- Rendering -----

function layout() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  const n = statuses.length || 1;
  // Leave room for the timeout labels at the top and the hints at the bottom.
  const top = 20;
  const bottom = 52;
  const cy = top + (h - top - bottom) / 2;
  const ring = Math.min(w, h - top - bottom) * 0.38;
  const radius = Math.max(16, Math.min(38, (2 * Math.PI * ring) / n / 3.2));
  const positions = statuses.map((_, i) => {
    const a = -Math.PI / 2 + (2 * Math.PI * i) / n;
    return { x: w / 2 + ring * Math.cos(a), y: cy + ring * Math.sin(a) };
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
  const panel = css.getPropertyValue('--node').trim();
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

  const realNow = performance.now();
  effects = effects.filter((e) => e.until > realNow);

  // Decision: a green wave rolls out from the center.
  const center = positions.length > 1
    ? { x: positions.reduce((t, p) => t + p.x, 0) / positions.length, y: positions.reduce((t, p) => t + p.y, 0) / positions.length }
    : { x: w / 2, y: h / 2 };
  const ringRadius = positions.length > 1 ? Math.hypot(positions[0].x - center.x, positions[0].y - center.y) : 60;
  for (const e of effects) {
    if (e.kind !== 'burst') continue;
    const k = 1 - (e.until - realNow) / 1100;
    for (const lag of [0, 0.18]) {
      const kk = Math.max(0, k - lag);
      if (kk <= 0) continue;
      ctx.strokeStyle = STEP_COLORS.Commit;
      ctx.globalAlpha = 0.45 * (1 - kk);
      ctx.lineWidth = 6 * (1 - kk) + 1;
      ctx.beginPath();
      ctx.arc(center.x, center.y, kk * (ringRadius + radius + 30), 0, 2 * Math.PI);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
  }

  // Polka: the links between the prevoters glow.
  for (const e of effects) {
    if (e.kind !== 'polka') continue;
    const k = 1 - (e.until - realNow) / 900;
    ctx.strokeStyle = STEP_COLORS.Prevote;
    ctx.shadowColor = STEP_COLORS.Prevote;
    ctx.shadowBlur = 12;
    ctx.lineWidth = 2.5;
    ctx.globalAlpha = Math.min(1, 4 * (1 - k)) * 0.8;
    for (let a = 0; a < e.voters.length; a++) {
      for (let b = a + 1; b < e.voters.length; b++) {
        const p = positions[e.voters[a]];
        const q = positions[e.voters[b]];
        if (!p || !q) continue;
        ctx.beginPath();
        ctx.moveTo(p.x, p.y);
        ctx.lineTo(q.x, q.y);
        ctx.stroke();
      }
    }
    ctx.shadowBlur = 0;
    ctx.globalAlpha = 1;
  }

  // Sync: a dashed beam from the node catching up to the peer it asks.
  for (const e of effects) {
    if (e.kind !== 'beam') continue;
    const p = positions[e.from];
    const q = positions[e.to];
    if (!p || !q) continue;
    const k = 1 - (e.until - realNow) / 1800;
    ctx.strokeStyle = MSG_COLORS.SyncRequest;
    ctx.globalAlpha = Math.min(1, 3 * (1 - k));
    ctx.lineWidth = 2;
    ctx.setLineDash([6, 6]);
    ctx.lineDashOffset = -realNow / 30;
    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
    ctx.lineTo(q.x, q.y);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.lineDashOffset = 0;
    ctx.globalAlpha = 1;
  }

  // Messages travel along slightly curved paths: A->B and B->A bend to opposite
  // sides, so both directions stay visible.
  inflight = inflight.filter((m) => simTime < m.deliver_at);
  for (const m of inflight) {
    if (!shown.has(m.kind)) continue;
    const a = positions[m.from];
    const b = positions[m.to];
    if (!a || !b) continue;
    const span = Math.max(1, m.deliver_at - m.time);
    const p = Math.min(1, Math.max(0, (simTime - m.time) / span));

    const dx = b.x - a.x;
    const dy = b.y - a.y;
    const len = Math.hypot(dx, dy) || 1;
    const bend = Math.min(40, len * 0.12);
    const cx = (a.x + b.x) / 2 - (dy / len) * bend;
    const cy = (a.y + b.y) / 2 + (dx / len) * bend;
    const edge = (radius + 6) / len;
    const at = (t) => {
      const u = edge + (1 - 2 * edge) * Math.min(1, Math.max(0, t));
      const v = 1 - u;
      return [v * v * a.x + 2 * v * u * cx + u * u * b.x, v * v * a.y + 2 * v * u * cy + u * u * b.y];
    };

    const color = MSG_COLORS[m.kind];
    const big = m.kind === 'Proposal' || m.kind.startsWith('Sync');
    const r = big ? 5.5 : 3.5;

    // A dropped message gets halfway, then fizzles out in a few sparks.
    if (m.dropped && p > 0.5) {
      const [x, y] = at(0.5);
      const k = (p - 0.5) * 2;
      ctx.globalAlpha = 1 - k;
      ctx.fillStyle = color;
      for (let j = 0; j < 6; j++) {
        const ang = (j / 6) * 2 * Math.PI + m.time;
        const d = 3 + k * 16;
        ctx.beginPath();
        ctx.arc(x + Math.cos(ang) * d, y + Math.sin(ang) * d, 1.8 * (1 - k) + 0.4, 0, 2 * Math.PI);
        ctx.fill();
      }
      ctx.strokeStyle = '#d93025';
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(x - 4, y - 4); ctx.lineTo(x + 4, y + 4);
      ctx.moveTo(x + 4, y - 4); ctx.lineTo(x - 4, y + 4);
      ctx.stroke();
      ctx.globalAlpha = 1;
      continue;
    }

    const t = p;

    // Fading trail
    ctx.fillStyle = color;
    for (let j = 6; j >= 1; j--) {
      const tt = t - j * 0.022;
      if (tt <= 0) continue;
      const [tx, ty] = at(tt);
      ctx.globalAlpha = 0.28 * (1 - j / 7);
      ctx.beginPath();
      ctx.arc(tx, ty, r * (1 - j / 9), 0, 2 * Math.PI);
      ctx.fill();
    }
    ctx.globalAlpha = 1;

    const [x, y] = at(t);
    ctx.beginPath();
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

  const n = statuses.length;
  const q = quorum(n);

  statuses.forEach((s, i) => {
    const { y } = positions[i];
    let { x } = positions[i];

    // Nodes that time out shake briefly.
    for (const e of effects) {
      if (e.node !== i || e.kind !== 'shake') continue;
      const k = 1 - (e.until - realNow) / 450;
      x += Math.sin(k * Math.PI * 8) * 4 * (1 - k);
    }
    const target = s.online ? STEP_COLORS[s.step] || muted : muted;

    // Ease the ring color and the vote arcs, so that changes read as motion.
    const look = (nodeLook[i] ??= { rgb: hexToRgb(target), prevotes: 0, precommits: 0, alpha: 1 });
    look.rgb = look.rgb.map((c, k) => ease(c, hexToRgb(target)[k], 0.18));
    const round = Math.max(0, s.round);
    look.prevotes = ease(look.prevotes, votesSeen(i, s.height, round, 'Prevote') / n, 0.25);
    look.precommits = ease(look.precommits, votesSeen(i, s.height, round, 'Precommit') / n, 0.25);
    look.alpha = ease(look.alpha, s.online ? 1 : 0.45, 0.12);
    const color = rgb(look.rgb);

    // Arrival ripples
    for (const e of effects) {
      if (e.node !== i || e.kind !== 'ripple') continue;
      const k = 1 - (e.until - realNow) / 380;
      ctx.strokeStyle = e.color;
      ctx.globalAlpha = 0.5 * (1 - k);
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(x, y, radius + 2 + k * 10, 0, 2 * Math.PI);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    for (const e of effects) {
      if (e.node !== i || e.kind !== 'decided') continue;
      const k = 1 - (e.until - realNow) / 700;
      ctx.strokeStyle = STEP_COLORS.Commit;
      ctx.globalAlpha = 1 - k;
      ctx.lineWidth = 3;
      ctx.beginPath();
      ctx.arc(x, y, radius + 14 + k * 18, 0, 2 * Math.PI);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    ctx.globalAlpha = look.alpha;

    // Vote arcs: prevotes (inner) and precommits (outer) seen for the current round,
    // starting at the top and filling clockwise, with a tick at the quorum.
    if (s.online) {
      const arcs = [[radius + 7, look.prevotes, STEP_COLORS.Prevote], [radius + 12, look.precommits, STEP_COLORS.Precommit]];
      for (const [r, frac, c] of arcs) {
        const a0 = -Math.PI / 2;
        ctx.lineCap = 'round';
        ctx.lineWidth = 3;
        ctx.strokeStyle = border;
        ctx.beginPath();
        ctx.arc(x, y, r, 0, 2 * Math.PI);
        ctx.stroke();
        if (frac > 0.01) {
          ctx.strokeStyle = c;
          ctx.globalAlpha = look.alpha * (frac >= q / n - 1e-6 ? 1 : 0.65);
          ctx.beginPath();
          ctx.arc(x, y, r, a0, a0 + 2 * Math.PI * Math.min(1, frac));
          ctx.stroke();
          ctx.globalAlpha = look.alpha;
        }
        const aq = a0 + (2 * Math.PI * q) / n;
        ctx.strokeStyle = muted;
        ctx.lineWidth = 1.5;
        ctx.lineCap = 'butt';
        ctx.beginPath();
        ctx.moveTo(x + (r - 3) * Math.cos(aq), y + (r - 3) * Math.sin(aq));
        ctx.lineTo(x + (r + 3) * Math.cos(aq), y + (r + 3) * Math.sin(aq));
        ctx.stroke();
      }
    }

    // The proposer of the current round glows.
    const proposing = s.online && proposerOf.get(i) === i;
    if (proposing) {
      ctx.shadowColor = STEP_COLORS.Propose;
      ctx.shadowBlur = 22;
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
    ctx.shadowBlur = 0;

    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    ctx.fillStyle = s.online ? fg : muted;
    ctx.font = `600 ${Math.round(radius * 0.42)}px "Space Grotesk", system-ui, sans-serif`;
    ctx.fillText(`v${i}`, x, y - radius * 0.18);
    ctx.font = `${Math.round(radius * 0.32)}px ui-monospace, Menlo, monospace`;
    ctx.fillStyle = muted;
    ctx.fillText(`H${s.height} R${round}`, x, y + radius * 0.3);

    // Step label below the node
    ctx.font = '12px system-ui, sans-serif';
    ctx.fillStyle = color;
    ctx.fillText(s.online ? s.step : 'offline', x, y + radius + 26);

    // Highlighted from the chain or the log
    if (hoverNodes.has(i)) {
      ctx.strokeStyle = fg;
      ctx.lineWidth = 2;
      ctx.setLineDash([4, 5]);
      ctx.lineDashOffset = -realNow / 40;
      ctx.beginPath();
      ctx.arc(x, y, radius + 19, 0, 2 * Math.PI);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.lineDashOffset = 0;
    }

    // Offline nodes doze off.
    if (!s.online) {
      const t = realNow / 900;
      ctx.fillStyle = muted;
      for (let k = 0; k < 3; k++) {
        const phase = (t + k / 3) % 1;
        ctx.globalAlpha = look.alpha * Math.sin(Math.PI * phase);
        ctx.font = `600 ${10 + k * 3}px system-ui, sans-serif`;
        ctx.fillText('z', x + radius * 0.7 + phase * 14, y - radius * 0.7 - phase * 22);
      }
    }
    ctx.globalAlpha = 1;

    // Proposer badge
    if (proposing) {
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
      ctx.fillText(e.label, x, y - radius - 24 - k * 14 - age * 10);
      ctx.globalAlpha = 1;
    });
  });
}

function renderChain() {
  const key = statuses.map((s) => s.decisions.length).join(',');
  if (key === chainKey) return;
  chainKey = key;

  const MAX = 14;
  const before = chainCounts;
  chainCounts = statuses.map((s) => s.decisions.length);
  $('chainRows').innerHTML = statuses.map((s, i) => {
    const d = s.decisions;
    const from = Math.max(0, d.length - MAX);
    const blocks = d.slice(from).map((v, k) => {
      const height = from + k + 1;
      const proposer = v % 100;
      const round = decidedHeights.get(height)?.round ?? 0;
      const classes = ['block'];
      if (height > (before[i] ?? 0)) classes.push('new');
      if (height === hoverHeight) classes.push('hl');
      const badge = round > 0 ? `<i class="rnd">R${round}</i>` : '';
      return `<span class="${classes.join(' ')}" data-height="${height}" data-proposer="${proposer}" ` +
        `style="background:${NODE_COLORS[proposer % NODE_COLORS.length]}" ` +
        `title="height ${height}, decided in round ${round}, proposed by v${proposer}">${v}${badge}</span>`;
    }).join('');
    const more = from > 0 ? `<span class="block more">… ${from} more</span>` : '';
    return `<div class="chainrow"><span class="who" style="color:${NODE_COLORS[i % NODE_COLORS.length]}">v${i}</span>${more}${blocks}</div>`;
  }).join('');
}

function renderStats() {
  const maxHeight = statuses.reduce((m, s) => Math.max(m, s.decisions.length), 0);
  const dropPct = counters.sent ? ((100 * counters.dropped) / counters.sent).toFixed(1) : '0.0';
  $('stats').innerHTML =
    `<span>t = <b>${(simTime / 1000).toFixed(2)} s</b></span>` +
    `<span><b>${maxHeight}</b> heights decided</span>` +
    `<span><b>${counters.sent}</b> messages, <b>${dropPct}%</b> dropped</span>`;
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
