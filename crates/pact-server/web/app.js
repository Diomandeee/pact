// PACT — the page.
//
// No framework and no build step. Five fetches, some DOM, and the same shape
// rules `pact export` applies to Mermaid: a source is a circle, a drain is a
// trapezoid, a pool under pressure is ringed. Shape carries the diagnosis
// because the whole argument of this tool is that structure is legible.

const $ = (s) => document.querySelector(s);
const el = (tag, cls, text) => {
  const n = document.createElement(tag);
  if (cls) n.className = cls;
  if (text !== undefined) n.textContent = text;
  return n;
};

async function api(path) {
  const r = await fetch(path);
  if (!r.ok) throw new Error(`${path} answered ${r.status}`);
  return r.json();
}

// ─────────────────────────────────────────────── navigation

document.querySelectorAll('nav button').forEach((b) => {
  b.onclick = () => {
    document.querySelectorAll('nav button').forEach((x) => x.classList.remove('on'));
    document.querySelectorAll('.view').forEach((x) => x.classList.remove('on'));
    b.classList.add('on');
    $('#' + b.dataset.view).classList.add('on');
  };
});

$('#close').onclick = () => { $('#detail').hidden = true; };
document.addEventListener('keydown', (e) => {
  if (e.key === 'Escape') $('#detail').hidden = true;
});

// ─────────────────────────────────────────────── health

function health(o) {
  $('#n-skills').textContent = o.skills;
  $('#n-types').textContent = o.types;
  $('#n-loops').textContent = o.feedback_loops;
  $('#n-dead').textContent = o.never_fires.length;
  // The only figure that turns red, and only when it is not zero. A dashboard
  // where several things are always coloured has no way left to alarm.
  if (o.never_fires.length > 0) $('#dead-fig').classList.add('alarm');
  $('#verdict').textContent = o.verdict;

  const rows = $('#pressure tbody');
  o.pressure.forEach((p) => {
    const tr = el('tr');
    tr.append(el('td', '', p.ty));
    tr.append(el('td', 'num hot', String(p.pressure)));
    tr.append(el('td', 'num', `${p.producers} in / ${p.consumers} out`));
    rows.append(tr);
  });
  if (o.pressure.length === 0) {
    rows.append(el('tr')).append(el('td', 'empty', 'nothing accumulating'));
  }

  o.source_only.forEach((t) => $('#sources').append(el('li', '', t)));
  o.dead_ends.forEach((t) => $('#deadends').append(el('li', '', t)));
}

// ─────────────────────────────────────────────── chains

function chains(c) {
  const body = $('#chain-table tbody');
  // Most complete first: the useful order is "what is closest to working".
  const sorted = [...c.chains].sort((a, b) => b.present - a.present || a.name.localeCompare(b.name));

  sorted.forEach((ch) => {
    const tr = el('tr');
    const score = el('td', 'score' + (ch.present === 4 ? ' full' : ''), `${ch.present}/4`);
    tr.append(score);
    tr.append(el('td', '', ch.name));
    tr.append(el('td', 'word', ch.word));

    const missing = el('td');
    if (ch.missing.length === 0) missing.append(el('span', 'empty', 'nothing'));
    ch.missing.forEach((m) => missing.append(el('span', 'pill', m)));
    tr.append(missing);

    const types = el('td');
    types.append(el('span', 'pill' + (ch.typechecks ? '' : ' broken'),
      ch.typechecks ? 'composes' : 'breaks'));
    tr.append(types);
    body.append(tr);
  });

  const short = $('#short tbody');
  c.missing_across_corpus.forEach(([stage, count]) => {
    const tr = el('tr');
    tr.append(el('td', 'word', stage));
    tr.append(el('td', 'num' + (count > 0 ? ' hot' : ''), String(count)));
    tr.append(el('td', '', count === 1 ? 'chain is short of it' : 'chains are short of it'));
    short.append(tr);
  });
}

// ─────────────────────────────────────────────── flow

function flow(g) {
  const host = $('#graph');
  const widest = Math.max(1, ...g.nodes.map((n) => n.inflow));

  g.nodes.forEach((n) => {
    const row = el('div', 'tnode' + (n.under_pressure ? ' hot' : ''));

    // The same shape vocabulary as `pact export`. A node that is a stadium in
    // one view and a box in the other would make the two disagree about a
    // diagnosis they both computed.
    const glyph = el('span', 'glyph ' +
      (n.kind === 'source' ? 'k-source' : n.kind === 'drain' ? 'k-drain'
        : n.under_pressure ? 'k-hot' : 'k-pool'));
    glyph.dataset.kind = n.kind;
    if (n.under_pressure) glyph.dataset.pressure = 'true';
    row.append(glyph);

    const mid = el('div');
    mid.append(el('div', 'name', n.id));
    const bar = el('div', 'bar' + (n.under_pressure ? ' hot' : ''));
    bar.style.width = `${Math.max(2, (n.inflow / widest) * 100)}%`;
    mid.append(bar);
    row.append(mid);

    row.append(el('div', 'flow',
      n.pressure === null ? `${n.inflow} in / 0 out` : `${n.inflow} / ${n.outflow}  ×${n.pressure}`));
    host.append(row);
  });

  // Skills, grouped under the type they read, as the arrows they are.
  const byInput = {};
  g.edges.forEach((e) => { (byInput[e.from] ||= []).push(e); });
  Object.keys(byInput).sort().forEach((from) => {
    const row = el('div', 'tnode');
    row.append(el('span'));
    const mid = el('div');
    mid.append(el('div', 'name', `${from} →`));
    const list = el('div');
    byInput[from].sort((a, b) => a.skill.localeCompare(b.skill)).forEach((e) => {
      const b = el('button', 'skill' + (e.dead ? ' dead' : ''), `${e.skill} → ${e.to}`);
      b.dataset.skill = e.skill;
      if (e.dead) b.dataset.dead = 'true';
      b.onclick = () => detail(e.skill);
      list.append(b, el('span', '', '  '));
    });
    mid.append(list);
    row.append(mid);
    row.append(el('div', 'flow', `${byInput[from].length}`));
    host.append(row);
  });
}

// ─────────────────────────────────────────────── cost

function cost(t) {
  $('#n-share').textContent = t.engine_share + '%';
  $('#n-engine').textContent = t.engine;
  $('#n-model').textContent = t.model;
  $('#n-agent').textContent = t.agent;

  const body = $('#reasons tbody');
  if (t.reasons.length === 0) {
    body.append(el('tr')).append(el('td', 'empty', 'nothing escalated'));
  }
  t.reasons.forEach(([reason, count]) => {
    const tr = el('tr');
    tr.append(el('td', 'num', String(count)));
    tr.append(el('td', '', reason));
    body.append(tr);
  });
}

// ─────────────────────────────────────────────── skill detail

async function detail(name) {
  const panel = $('#detail');
  const body = $('#detail-body');
  body.replaceChildren(el('p', 'note', 'reading…'));
  panel.hidden = false;

  let s;
  try {
    s = await api('/api/skill/' + encodeURIComponent(name));
  } catch (e) {
    // A 404 is a real answer and says something different from a failure.
    body.replaceChildren(el('h3', '', name), el('p', 'fail', 'no such skill'));
    return;
  }

  body.replaceChildren();
  body.append(el('h3', '', s.name));
  body.append(el('div', 'sig', `${s.input} → ${s.outputs.join(' | ')}`));

  if (!s.can_fire) {
    body.append(el('p', 'fail', 'Nothing produces what it reads, so it can never fire.'));
  }
  if (s.category) body.append(el('span', 'pill', s.category));
  if (s.idempotent) body.append(el('span', 'pill', 'idempotent'));

  const section = (title, items, note) => {
    body.append(el('h2', '', title));
    if (note) body.append(el('p', 'note', note));
    if (items.length === 0) { body.append(el('p', 'empty', 'none')); return; }
    const ul = el('ul');
    items.forEach((i) => {
      const li = el('li');
      const b = el('button', 'skill', i);
      b.onclick = () => detail(i);
      li.append(b);
      ul.append(li);
    });
    body.append(ul);
  };

  section('Can follow', s.upstream, 'Skills whose output this one reads.');
  section('Can precede', s.downstream, 'Skills that read what this one emits.');
  body.append(el('h2', '', 'In chains'));
  if (s.in_chains.length === 0) body.append(el('p', 'empty', 'no chain names it'));
  else s.in_chains.forEach((c) => body.append(el('div', 'word', c)));
}

// ─────────────────────────────────────────────── boot

(async () => {
  try {
    // The headline is already in the HTML, server-rendered. These fetches
    // fill in everything else and confirm the page and the server agree.
    health(await api('/api/overview'));
    chains(await api('/api/chains'));
    flow(await api('/api/graph'));
    cost(await api('/api/tiers'));
  } catch (e) {
    $('#root').textContent = 'could not read the corpus: ' + e.message;
    $('#root').classList.add('fail');
  }
})();
