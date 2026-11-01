import init, { WasmSim } from './pkg/arc_malachitebft_wasm_sim.js';

const STEP_COLORS = {
  Unstarted: '#9a948c',
  Propose: '#2f5fd0',
  Prevote: '#d4a017',
  Precommit: '#8e5bd0',
  Commit: '#c8692f',
};

const MSG_COLORS = {
  Proposal: '#2f5fd0',
  Prevote: '#d4a017',
  Precommit: '#8e5bd0',
  PolkaCertificate: '#d1457f',
  RoundCertificate: '#c0392b',
  Status: '#9a948c',
  SyncRequest: '#1fb5b0',
  SyncResponse: '#1fb5b0',
};

const LEGEND_LABELS = {
  Proposal: 'Proposal',
  Prevote: 'Prevote',
  Precommit: 'Precommit',
  PolkaCertificate: 'Polka cert',
  RoundCertificate: 'Round cert',
  Status: 'Status',
  SyncRequest: 'Sync req',
  SyncResponse: 'Sync resp',
};

// Tooltips for the message filters and the step legend.
const MSG_TIPS = {
  Proposal: 'The round\'s proposer sends the value it proposes for this height to every validator. Drawn as an arrowhead pointing the way it travels.',
  Prevote: 'A validator\'s first vote in a round: for the proposed value if it is valid and the validator is not locked on another one, otherwise nil. Hollow when nil.',
  Precommit: 'A validator\'s second vote: for a value once it has seen prevotes for it from more than 2/3 of the validators, otherwise nil. Precommits for one value from more than 2/3 decide it. Hollow when nil.',
  PolkaCertificate: 'Prevotes for one value from more than 2/3 of the validators, bundled together. Helps validators that missed some of those prevotes catch up.',
  RoundCertificate: 'Proof that a round is over, from precommits of more than 2/3 of the validators, or proof that more than 1/3 have moved to a later round. Lets validators that are behind jump to the right round.',
  Status: 'Every validator regularly tells its peers the latest height it decided, so that a validator that fell behind finds out. Hidden by default, as there are a lot of them.',
  SyncRequest: 'A validator that fell behind asks a peer that is ahead for the value decided at its current height. Drawn as an empty ring. Never dropped.',
  SyncResponse: 'The decided value and the commit certificate that proves it. The validator that asked checks it and decides that height. Shaped like a block in the chains panel. Never dropped.',
};

const STEP_TIPS = {
  Propose: 'Waiting for the round\'s proposal, or building it when it is this validator\'s turn, marked with a P.',
  Prevote: 'Has sent its prevote, and collects the others\' prevotes.',
  Precommit: 'Has sent its precommit, and collects the others\' precommits.',
  Commit: 'Has decided the height. Grey while waiting for the target block time before moving on.',
};

// Identity colors, used to show who proposed each decided value.
const NODE_COLORS = [
  '#2f5fd0', '#1f9d6a', '#c8692f', '#1fb5b0', '#d9a21b', '#b33a3a', '#6f8fe0', '#5cbf8a',
  '#e0895a', '#147a7a', '#8a6cc9', '#9cbf3a', '#7a4a2a', '#3a8fd0', '#c94f7c', '#5a6e7a',
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
let heightStart = []; // node -> virtual time at which it started its current height
let counters = { sent: 0, dropped: 0 };
let playing = true;
let simTime = 0;
let lastFrame = performance.now();
let chainKey = '';
// Statuses are sent all the time, so they are hidden unless asked for.
const HIDDEN_BY_DEFAULT = ['Status'];
const shown = new Set(Object.keys(MSG_COLORS).filter((kind) => !HIDDEN_BY_DEFAULT.includes(kind)));

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

const statusInterval = () => Math.max(100, Math.round(+$('statusInterval').value || 0));
const syncGrace = () => Math.max(0, Math.round(+$('syncGrace').value || 0));
const targetBlockTime = () => Math.max(0, Math.round(+$('targetBlockTime').value || 0));

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
    status_interval_ms: statusInterval(),
    sync_grace_ms: syncGrace(),
    target_block_time_ms: targetBlockTime(),
  }));

  simTime = 0;
  inflight = [];
  effects = [];
  proposerOf = new Map();
  decidedHeights = new Map();
  heightStart = [];
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

const ICONS = {
  play: '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M4 2.8v10.4a.8.8 0 0 0 1.2.7l7.6-5.2a.8.8 0 0 0 0-1.4L5.2 2.1a.8.8 0 0 0-1.2.7z"/></svg>',
  pause: '<svg viewBox="0 0 16 16" aria-hidden="true"><rect x="3.5" y="2.5" width="3" height="11" rx="1"/><rect x="9.5" y="2.5" width="3" height="11" rx="1"/></svg>',
};

$('play').onclick = () => {
  playing = !playing;
  const label = playing ? 'Pause' : 'Play';
  $('play').innerHTML = playing ? ICONS.pause : ICONS.play;
  $('play').title = label;
  $('play').setAttribute('aria-label', label);
};
$('stepBtn').onclick = () => advance(100);
$('reset').onclick = reset;
$('nodes').onchange = reset;
$('seed').onchange = reset;
for (const f of TIMEOUT_FIELDS) {
  $('t_' + f).onchange = () => sim.set_timeouts(JSON.stringify(timeouts()));
}
$('statusInterval').onchange = () => sim.set_status_interval_ms(statusInterval());
$('syncGrace').onchange = () => sim.set_sync_grace_ms(syncGrace());
$('targetBlockTime').onchange = () => sim.set_target_block_time_ms(targetBlockTime());
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

$('log').addEventListener('mouseover', (e) => {
  const entry = e.target.closest('.entry');
  if (!entry) return;
  hoverNodes = new Set(entry.dataset.nodes ? entry.dataset.nodes.split(',').map(Number) : []);
});
$('log').addEventListener('mouseleave', () => { hoverNodes = new Set(); });

// Tooltips for the elements with a `data-tip`, placed beside them and kept on screen.
function showTip(el) {
  const tip = $('tip');
  tip.textContent = el.dataset.tip;
  tip.hidden = false;
  const r = el.getBoundingClientRect();
  const t = tip.getBoundingClientRect();
  const m = 8;
  // Beside the settings panel when there is room, so the inputs stay visible.
  let x = (el.closest('#settings') ?? el).getBoundingClientRect().right + 10;
  let y = r.top + r.height / 2 - t.height / 2;
  if (x + t.width > innerWidth - m) {
    // No room on the right: go below instead.
    x = Math.min(Math.max(m, r.left), innerWidth - t.width - m);
    y = r.bottom + 8;
    if (y + t.height > innerHeight - m) y = r.top - t.height - 8;
  }
  tip.style.left = `${x}px`;
  tip.style.top = `${Math.min(Math.max(m, y), innerHeight - t.height - m)}px`;
}
const hideTip = () => { $('tip').hidden = true; };
document.addEventListener('pointerover', (e) => {
  const el = e.target.closest('[data-tip]');
  if (el) showTip(el); else hideTip();
});
document.addEventListener('focusin', (e) => {
  const el = e.target.closest('[data-tip]') ?? document.querySelector(`label[for="${e.target.id}"][data-tip]`);
  if (el) showTip(el); else hideTip();
});
document.addEventListener('focusout', hideTip);
$('settings').addEventListener('scroll', hideTip);

$('clearLog').onclick = () => $('log').replaceChildren();

// Message filters, as toggle chips over the canvas.
for (const [kind, color] of Object.entries(MSG_COLORS)) {
  const chip = document.createElement('button');
  chip.className = 'chip';
  chip.dataset.kind = kind;
  chip.dataset.tip = `${MSG_TIPS[kind]} Click to show or hide them.`;
  chip.setAttribute('aria-pressed', 'true');
  const shape = kind.endsWith('Certificate') ? ' square' : { Proposal: ' tri', SyncRequest: ' ring', SyncResponse: ' pill' }[kind] ?? '';
  chip.innerHTML = `<span class="sw${shape}" style="--c:${color}"></span>${LEGEND_LABELS[kind]}`;
  chip.onclick = () => {
    setShown(kind, !shown.has(kind));
    saveSettings();
  };
  $('legend').appendChild(chip);
  setShown(kind, shown.has(kind));
}

function setShown(kind, on) {
  if (on) shown.add(kind); else shown.delete(kind);
  const chip = document.querySelector(`.chip[data-kind="${kind}"]`);
  chip.classList.toggle('off', !on);
  chip.setAttribute('aria-pressed', String(on));
}

// Legend for the node ring colors.
for (const step of ['Propose', 'Prevote', 'Precommit', 'Commit']) {
  const item = document.createElement('span');
  item.innerHTML = `<i class="disc" style="--c:${STEP_COLORS[step]}"></i>${step}`;
  item.dataset.tip = STEP_TIPS[step];
  $('steps').appendChild(item);
}
{
  const item = document.createElement('span');
  item.textContent = 'Dots: votes seen';
  item.dataset.tip = 'Under each validator, one dot per validator, from v0 on the left: the top row shows the prevotes it has seen in the current round, the bottom row the precommits. Filled for a vote for a value, hollow for nil, faint until the vote arrives. A row brightens and gets a check mark once it holds a quorum: votes from more than 2/3 of the validators.';
  $('steps').prepend(item);
}

canvas.addEventListener('click', (e) => {
  const rect = canvas.getBoundingClientRect();
  const x = e.clientX - rect.left;
  const y = e.clientY - rect.top;
  const { positions, radius } = layout();
  positions.forEach((p, i) => {
    if (Math.hypot(p.x - x, p.y - y) <= radius + 4) {
      setOnline(i, !statuses[i].online);
    }
  });
});

function setOnline(i, online) {
  if (statuses[i].online === online) return;
  sim.set_online(i, online);
  log(sim.now(), `v${i} ${online ? 'back online' : 'taken offline'}`, online ? '' : 'fault', [i]);
  statuses = JSON.parse(sim.statuses());
}

// ----- Scenarios -----

function setInput(id, value) {
  $(id).value = value;
  $(id).dispatchEvent(new Event('input'));
}

const SCENARIOS = {
  calm() {
    setInput('drop', 0);
    setInput('minDelay', 20);
    setInput('maxDelay', 200);
  },
  lossy() {
    setInput('drop', 25);
  },
  slow() {
    setInput('minDelay', 100);
    setInput('maxDelay', 1500);
  },
  crash() {
    // Round 0 of height h is proposed by v((h - 1) mod n).
    const n = statuses.length;
    const height = Math.max(...statuses.filter((s) => s.online).map((s) => s.height));
    setOnline(height % n, false);
  },
  noquorum() {
    const n = statuses.length;
    const online = statuses.filter((s) => s.online).map((s) => s.node);
    const keep = quorum(n) - 1;
    for (const i of online.slice(keep)) setOnline(i, false);
  },
  heal() {
    statuses.forEach((_, i) => setOnline(i, true));
  },
};

for (const button of document.querySelectorAll('[data-scenario]')) {
  button.onclick = () => SCENARIOS[button.dataset.scenario]();
}

canvas.addEventListener('mousemove', (e) => {
  const rect = canvas.getBoundingClientRect();
  const { positions, radius } = layout();
  const over = positions.some((p) => Math.hypot(p.x - (e.clientX - rect.left), p.y - (e.clientY - rect.top)) <= radius + 4);
  canvas.style.cursor = over ? 'pointer' : 'default';
});

// ----- Simulation -----

const LOG_LIMIT = 300;

// Newest entries first. `kind` is one of '', 'decided', 'round', 'sync', 'fault'.
// Hovering an entry highlights the `nodes` it is about.
function log(time, text, kind = '', nodes = []) {
  const el = $('log');
  const entry = document.createElement('div');
  entry.className = `entry ${kind}`;
  entry.dataset.nodes = nodes.join(',');
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
        log(ev.time, `v${ev.from} is behind, requests height ${ev.height} from v${ev.to}`, 'sync', [ev.from, ev.to]);
        effects.push({ node: -1, kind: 'beam', from: ev.from, to: ev.to, until: realNow + 1800 });
      }
      break;
    case 'RoundStarted':
      proposerOf.set(ev.node, ev.proposer);
      if (ev.round === 0) heightStart[ev.node] = ev.time;
      if (ev.round > 0 && ev.node === ev.proposer) {
        log(ev.time, `height ${ev.height} moves to round ${ev.round}, v${ev.proposer} proposes`, 'round', [ev.proposer]);
      }
      if (ev.round > 0 && once(`round|${ev.height}|${ev.round}`)) {
        banner(`Height ${ev.height} · Round ${ev.round}`, `v${ev.proposer} proposes`);
      }
      break;
    case 'TimeoutElapsed':
      // The end of a finalization period is not a failure: no label, no shake.
      if (ev.kind !== 'Rebroadcast' && ev.kind !== 'FinalizeHeight') {
        effects.push({ node: ev.node, kind: 'timeout', label: `timeout ${ev.kind.toLowerCase()}`, until: realNow + 1200 });
        effects.push({ node: ev.node, kind: 'shake', until: realNow + 450 });
      }
      break;
    case 'Decided':
      effects.push({ node: ev.node, kind: 'decided', until: realNow + 700 });
      if (!decidedHeights.has(ev.height)) {
        decidedHeights.set(ev.height, ev);
        effects.push({ node: -1, kind: 'burst', until: realNow + 1100 });
        log(ev.time, `height ${ev.height} decided: value ${ev.value} (round ${ev.round}, proposed by v${ev.value % 100})`, 'decided', [ev.value % 100]);
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
  const bottom = 58;
  const cy = top + (h - top - bottom) / 2;
  const ring = Math.min(w, h - top - bottom) * 0.38;
  const radius = Math.max(14, Math.min(38, ring * 0.22, (2 * Math.PI * ring) / n / 3.2));
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
  const fault = css.getPropertyValue('--fault').trim();
  const warn = css.getPropertyValue('--warn').trim();

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
    const r = big ? 5.5 : m.kind === 'Status' ? 2.5 : 3.5;

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
      ctx.strokeStyle = fault;
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
    const nilVote = m.value === null && (m.kind === 'Prevote' || m.kind === 'Precommit');
    if (m.kind === 'SyncRequest' || nilVote) {
      // A sync request asks for a block, and a nil vote is for no value: both are empty rings
      const thick = m.kind === 'SyncRequest' ? 2 : 1.5;
      ctx.arc(x, y, r - thick / 2, 0, 2 * Math.PI);
      ctx.strokeStyle = color;
      ctx.lineWidth = thick;
      ctx.stroke();
      continue;
    }
    if (m.kind === 'Proposal') {
      // An arrowhead pointing the way it travels
      const [x0, y0] = at(t - 0.01);
      const [x1, y1] = at(t + 0.01);
      const ang = Math.atan2(y1 - y0, x1 - x0);
      const point = (dx, dy) => [x + dx * Math.cos(ang) - dy * Math.sin(ang), y + dx * Math.sin(ang) + dy * Math.cos(ang)];
      ctx.moveTo(...point(1.4 * r, 0));
      ctx.lineTo(...point(-r, r));
      ctx.lineTo(...point(-r, -r));
      ctx.closePath();
    } else if (m.kind === 'SyncResponse') {
      // Carries a block: shaped like the blocks in the chains panel
      ctx.roundRect(x - 7, y - 4.5, 14, 9, 3);
    } else if (m.kind.endsWith('Certificate')) {
      ctx.rect(x - r, y - r, 2 * r, 2 * r);
    } else {
      ctx.arc(x, y, r, 0, 2 * Math.PI);
    }
    ctx.fill();
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
    // A node that decided before the target block time waits for it, shown in grey.
    const targetMs = targetBlockTime();
    const waitLeft = s.online && s.step === 'Commit' && targetMs > 0 && heightStart[i] !== undefined
      ? heightStart[i] + targetMs - simTime
      : 0;
    const waiting = waitLeft > 0;
    const target = s.online && !waiting ? STEP_COLORS[s.step] || muted : muted;

    // Ease the color, so that step changes read as motion.
    const look = (nodeLook[i] ??= { rgb: hexToRgb(target), alpha: 1 });
    look.rgb = look.rgb.map((c, k) => ease(c, hexToRgb(target)[k], 0.18));
    const round = Math.max(0, s.round);
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

    // Vote pips: one per validator, left to right, for the prevotes (top row) and
    // precommits (bottom row) seen in the current round. Filled for a value, hollow
    // for nil, faint until the vote arrives. A row brightens once it is a quorum.
    if (s.online) {
      const gap = Math.min(9, (2 * radius + 16) / n);
      const r = Math.max(1, Math.min(3, gap * 0.34));
      const x0 = x - (gap * (n - 1)) / 2;
      const rows = [['Prevote', y + radius + 9], ['Precommit', y + radius + 18]];
      for (const [kind, py] of rows) {
        const seen = votes.get(`${i}|${s.height}|${round}|${kind}`) ?? new Map();
        const full = seen.size >= q;
        for (let v = 0; v < n; v++) {
          const px = x0 + v * gap;
          ctx.beginPath();
          ctx.arc(px, py, r, 0, 2 * Math.PI);
          if (!seen.has(v)) {
            ctx.fillStyle = border;
            ctx.fill();
            continue;
          }
          ctx.globalAlpha = look.alpha * (full ? 1 : 0.6);
          if (seen.get(v) === null) {
            ctx.strokeStyle = STEP_COLORS[kind];
            ctx.lineWidth = 1.2;
            ctx.beginPath();
            ctx.arc(px, py, r - 0.6, 0, 2 * Math.PI);
            ctx.stroke();
          } else {
            ctx.fillStyle = STEP_COLORS[kind];
            ctx.fill();
          }
          ctx.globalAlpha = look.alpha;
        }
        if (full) {
          // A check mark after the row
          const cx = x0 + n * gap + 1;
          const k = Math.max(3, r * 1.4);
          ctx.strokeStyle = STEP_COLORS[kind];
          ctx.lineWidth = 1.6;
          ctx.lineCap = 'round';
          ctx.lineJoin = 'round';
          ctx.beginPath();
          ctx.moveTo(cx - k * 0.5, py);
          ctx.lineTo(cx - k * 0.1, py + k * 0.45);
          ctx.lineTo(cx + k * 0.7, py - k * 0.55);
          ctx.stroke();
          ctx.lineCap = 'butt';
          ctx.lineJoin = 'miter';
        }
      }
    }

    // The proposer of the current round glows.
    const proposing = s.online && proposerOf.get(i) === i;
    if (proposing) {
      ctx.shadowColor = STEP_COLORS.Propose;
      ctx.shadowBlur = 22;
    }
    // The step shows as a tint of the disc, inside a thin outline of the same color.
    ctx.fillStyle = panel;
    ctx.beginPath();
    ctx.arc(x, y, radius, 0, 2 * Math.PI);
    ctx.fill();
    ctx.shadowBlur = 0;
    if (s.online) {
      ctx.fillStyle = color;
      ctx.globalAlpha = look.alpha * 0.22;
      ctx.fill();
      ctx.globalAlpha = look.alpha;
    }
    ctx.strokeStyle = color;
    ctx.lineWidth = 2;
    ctx.setLineDash(s.online ? [] : [5, 4]);
    ctx.stroke();
    ctx.setLineDash([]);

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
    let label = s.online ? s.step : 'offline';
    if (waiting) label = `waiting ${(waitLeft / 1000).toFixed(1)} s`;
    ctx.fillText(label, x, y + radius + 32);

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
      ctx.fillStyle = warn;
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

function setStat(id, text, pulse = false) {
  const el = $(id);
  if (el.textContent === text) return;
  el.textContent = text;
  if (pulse) {
    el.classList.remove('pulse');
    void el.offsetWidth; // restart the animation
    el.classList.add('pulse');
  }
}

function renderStats() {
  const maxHeight = statuses.reduce((m, s) => Math.max(m, s.decisions.length), 0);
  const dropPct = counters.sent ? ((100 * counters.dropped) / counters.sent).toFixed(1) : '0.0';
  const rate = simTime > 0 ? maxHeight / (simTime / 1000) : 0;
  setStat('statTime', `${(simTime / 1000).toFixed(2)} s`);
  setStat('statHeights', String(maxHeight), maxHeight > 0);
  setStat('statRate', rate.toFixed(1));
  setStat('statMsgs', String(counters.sent));
  setStat('statDrop', `${dropPct}%`);

  // Time between the first decisions of the last two heights.
  const last = decidedHeights.get(maxHeight);
  const prev = decidedHeights.get(maxHeight - 1);
  setStat('statBlock', last && prev ? `${((last.time - prev.time) / 1000).toFixed(2)} s` : '–');
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

// ----- Settings persistence -----
// Every setting, the message filters and the open sections of the settings
// panel are remembered in this browser, and restored before the first run.

const SETTINGS_KEY = 'malachite-sim-settings';
const settingInputs = () => [$('speed'), ...document.querySelectorAll('#settings input[id]')];
const settingSections = () => [...document.querySelectorAll('#settings details')];

function saveSettings() {
  const settings = {
    values: Object.fromEntries(settingInputs().map((el) => [el.id, el.value])),
    hidden: Object.keys(MSG_COLORS).filter((kind) => !shown.has(kind)),
    shown: [...shown],
    closed: settingSections().filter((d) => !d.open).map((d) => d.querySelector('h2').textContent),
  };
  try { localStorage.setItem(SETTINGS_KEY, JSON.stringify(settings)); } catch { /* storage unavailable */ }
}

function restoreSettings() {
  let settings;
  try { settings = JSON.parse(localStorage.getItem(SETTINGS_KEY)); } catch { /* ignore */ }
  if (!settings) return;

  for (const el of settingInputs()) {
    const value = Number(settings.values?.[el.id]);
    if (settings.values?.[el.id] === undefined || !Number.isFinite(value)) continue;
    const min = el.min === '' ? -Infinity : +el.min;
    const max = el.max === '' ? Infinity : +el.max;
    el.value = String(Math.min(max, Math.max(min, value)));
  }
  // Message kinds saved neither as shown nor hidden keep their default.
  for (const kind of Object.keys(MSG_COLORS)) {
    if (settings.hidden?.includes(kind)) setShown(kind, false);
    if (settings.shown?.includes(kind)) setShown(kind, true);
  }
  for (const d of settingSections()) {
    d.open = !settings.closed?.includes(d.querySelector('h2').textContent);
  }
}

// Inputs changed by the scenario buttons fire non-bubbling events, so listen on each.
for (const el of settingInputs()) {
  el.addEventListener('input', saveSettings);
  el.addEventListener('change', saveSettings);
}
for (const d of settingSections()) d.addEventListener('toggle', saveSettings);

$('restoreDefaults').onclick = () => {
  try { localStorage.removeItem(SETTINGS_KEY); } catch { /* storage unavailable */ }
  location.reload();
};

// ----- Panel layout -----
// The log (right) and chains (bottom) panels can be resized by dragging their
// inner edge, and collapsed. The layout is remembered in this browser.

const main = document.querySelector('main');
const PANELS = {
  log: { prop: '--log-size', axis: 'x', min: 220, max: () => Math.min(720, innerWidth * 0.5), fallback: 320 },
  chain: { prop: '--chain-size', axis: 'y', min: 90, max: () => innerHeight * 0.6, fallback: 190 },
};
const LAYOUT_KEY = 'malachite-sim-layout';
let panelLayout = {};

function saveLayout() {
  try { localStorage.setItem(LAYOUT_KEY, JSON.stringify(panelLayout)); } catch { /* storage unavailable */ }
}

function setSize(name, px) {
  const p = PANELS[name];
  const size = Math.round(Math.min(p.max(), Math.max(p.min, px)));
  panelLayout[`${name}Size`] = size;
  main.style.setProperty(p.prop, `${size}px`);
}

function setCollapsed(name, collapsed) {
  panelLayout[`${name}Collapsed`] = collapsed;
  main.classList.toggle(`${name}-collapsed`, collapsed);
  const button = document.querySelector(`[data-fold="${name}"]`);
  button.setAttribute('aria-expanded', String(!collapsed));
  button.title = `${collapsed ? 'Expand' : 'Collapse'} the ${name === 'log' ? 'log' : 'chains'}`;
}

for (const button of document.querySelectorAll('[data-fold]')) {
  button.onclick = (e) => {
    e.stopPropagation();
    const name = button.dataset.fold;
    setCollapsed(name, !main.classList.contains(`${name}-collapsed`));
    saveLayout();
  };
}

// A collapsed panel expands when clicked anywhere.
for (const [name, id] of [['log', 'logPanel'], ['chain', 'chain']]) {
  $(id).addEventListener('click', () => {
    if (!main.classList.contains(`${name}-collapsed`)) return;
    setCollapsed(name, false);
    saveLayout();
  });
}

for (const handle of document.querySelectorAll('[data-resize]')) {
  const name = handle.dataset.resize;
  const p = PANELS[name];
  const panel = handle.parentElement;
  const current = () => (p.axis === 'x' ? panel.offsetWidth : panel.offsetHeight);

  handle.addEventListener('pointerdown', (e) => {
    e.preventDefault();
    handle.setPointerCapture(e.pointerId);
    handle.classList.add('dragging');
    document.body.classList.add('resizing', p.axis === 'x' ? 'col' : 'row');
    const start = p.axis === 'x' ? e.clientX : e.clientY;
    const size = current();
    // The panels sit at the end of their axis, so dragging towards the start grows them.
    const move = (ev) => setSize(name, size + start - (p.axis === 'x' ? ev.clientX : ev.clientY));
    const up = () => {
      handle.removeEventListener('pointermove', move);
      handle.classList.remove('dragging');
      document.body.classList.remove('resizing', 'col', 'row');
      saveLayout();
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up, { once: true });
    handle.addEventListener('pointercancel', up, { once: true });
  });

  handle.addEventListener('dblclick', () => {
    setSize(name, p.fallback);
    saveLayout();
  });

  handle.addEventListener('keydown', (e) => {
    const grow = p.axis === 'x' ? { ArrowLeft: 1, ArrowRight: -1 } : { ArrowUp: 1, ArrowDown: -1 };
    if (!(e.key in grow)) return;
    e.preventDefault();
    setSize(name, current() + grow[e.key] * (e.shiftKey ? 60 : 20));
    saveLayout();
  });
}

try { panelLayout = JSON.parse(localStorage.getItem(LAYOUT_KEY)) ?? {}; } catch { panelLayout = {}; }
for (const name of Object.keys(PANELS)) {
  if (panelLayout[`${name}Size`]) setSize(name, panelLayout[`${name}Size`]);
  setCollapsed(name, !!panelLayout[`${name}Collapsed`]);
}

await init();
restoreSettings();
syncOutputs();
reset();
requestAnimationFrame(frame);
