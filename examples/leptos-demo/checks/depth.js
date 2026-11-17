async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const text = (id) => document.getElementById(id).textContent;
  const within = (id) => document.getElementById(id).parentElement;
  const section = (title) => [...document.querySelectorAll('section')].find(s => s.querySelector('h3').textContent === title);
  const type = async (input, value) => { input.focus(); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await frame(); };
  const shown = (el) => !!el && getComputedStyle(el).display !== 'none';
  const who = () => document.activeElement.getAttribute('aria-label') || document.activeElement.dataset.value || document.activeElement.tagName;
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('comboboxLoading', async () => {
    const box = within('combo-value');
    const input = box.querySelector('input');
    const [status, empty] = ['Status', 'Empty'].map(name => box.querySelector(`[data-armature-part="${name}"]`));
    box.querySelector('[role=button]').click(); await frame();
    await type(input, 'zzz');
    const loading = `status=${shown(status)} empty=${shown(empty)} said="${document.getElementById('armature-live-polite')?.textContent}"`;
    box.querySelector('[role=button]').click(); await frame();
    return { loading, idle: `status=${shown(status)} empty=${shown(empty)}` };
  });

  await step('contextMenu', async () => {
    const box = within('menu-value');
    const area = box.querySelector('[data-armature-part="Area"]');
    const content = box.querySelector('[role=menu]');
    const at = area.getBoundingClientRect();
    const [x, y] = [Math.round(at.left + 30), Math.round(at.top + 12)];
    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: x, clientY: y });
    area.dispatchEvent(event); await frame(); await frame();
    const placed = content.getBoundingClientRect();
    const r = { opened: `shown=${shown(content)} browserMenuStopped=${event.defaultPrevented} atPointer=${Math.abs(placed.left - x) < 2 && Math.abs(placed.top - y) < 8} focus=${who()}` };
    document.activeElement.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })); await frame();
    r.closed = `shown=${shown(content)}`;
    return r;
  });

  await step('sidewaysScroll', async () => {
    const box = section('Scroll area').querySelector('.scroll-wide');
    const viewport = box.querySelector('[data-armature-part="Viewport"]');
    const place = box.querySelector('.thumb-across');
    await frame();
    const start = `left=${place.style.left} narrow=${parseFloat(place.style.width) < 150}`;
    viewport.scrollLeft = viewport.scrollWidth; await wait(150);
    const end = Math.round(parseFloat(place.style.left) + parseFloat(place.style.width));
    return { start, thumbReachesTheEnd: end === 192, orientation: box.querySelector('[role=scrollbar]').getAttribute('aria-orientation') };
  });

  await step('toastHotkey', async () => {
    const box = within('toast-value');
    const notify = box.querySelector('[role=button]');
    notify.focus(); notify.click(); await frame(); await wait(150);
    const press = () => { const e = new KeyboardEvent('keydown', { key: 'F6', bubbles: true, cancelable: true }); document.activeElement.dispatchEvent(e); };
    press(); await frame();
    const into = who();
    press(); await frame();
    return { into, back: who() };
  });

  await step('form', async () => {
    const box = within('form-value');
    const [name, team] = box.querySelectorAll('input');
    const button = (label) => [...box.querySelectorAll('[role=button]')].find(b => b.getAttribute('aria-label') === label);
    button('Create').click(); await frame();
    const r = { refused: `${text('form-value')} | ${text('form-name-error')}` };
    await type(name, 'Ada'); await type(team, 'Engines');
    r.fixed = `error="${text('form-name-error')}"`;
    button('Create').click(); await frame();
    r.sent = text('form-value');
    button('Reset').click(); await frame();
    r.reset = text('form-value');
    return r;
  });

  await step('safeTriangle', async () => {
    const box = within('nav-value');
    const trigger = box.querySelector('[data-armature-part="Trigger"]');
    const panel = box.querySelector('[data-armature-part="Panel"]');
    const hover = (el, type, x, y) => el.dispatchEvent(new PointerEvent(type, { bubbles: false, clientX: x, clientY: y, pointerType: 'mouse' }));
    const move = (x, y) => window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: x, clientY: y, pointerType: 'mouse' }));
    const t = trigger.getBoundingClientRect();
    hover(trigger, 'pointerenter', t.left + 5, t.top + 5); await wait(120);
    const p = panel.getBoundingClientRect();
    const opened = shown(panel);
    // Leave the trigger at its bottom edge and cross the gap towards the panel,
    // taking longer than the close delay.
    hover(trigger, 'pointerleave', t.left + 5, t.bottom);
    for (let i = 1; i <= 3; i++) { move(t.left + 5 + i * 4, t.bottom + (p.top - t.bottom) * i / 4); await wait(60); }
    const crossing = shown(panel);
    // Turn back instead of arriving.
    move(t.left + 5, t.top - 60); await wait(160);
    return { opened, staysOpenWhileCrossing: crossing, closesWhenThePointerTurnsAway: !shown(panel) };
  });

  await step('modality', async () => {
    const root = document.documentElement;
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); await frame();
    const pointer = root.getAttribute('data-armature-modality');
    document.body.dispatchEvent(new KeyboardEvent('keydown', { key: 'Tab', bubbles: true })); await frame();
    return `${pointer} > ${root.getAttribute('data-armature-modality')}`;
  });

  await step('remoteSearch', async () => {
    const box = within('remote-search-value');
    const input = box.querySelector('input');
    const status = box.querySelector('[data-armature-part="Status"]');
    await type(input, 'l'); await type(input, 'li');
    return `${text('remote-search-value')} stillSearching=${shown(status)}`;
  });

  await step('sheetWheel', async () => {
    const trigger = [...document.querySelectorAll('[data-armature-part="Trigger"]')].find(t => document.getElementById(t.getAttribute('aria-controls') || '')?.querySelector('[data-armature-part="Handle"]'));
    const content = document.getElementById(trigger.getAttribute('aria-controls'));
    const handle = content.querySelector('[data-armature-part="Handle"]');
    trigger.click(); await frame();
    const before = handle.getAttribute('aria-valuenow');
    const event = new WheelEvent('wheel', { deltaY: 400, bubbles: true, cancelable: true });
    content.dispatchEvent(event); await wait(400);
    return `height ${before} > ${handle.getAttribute('aria-valuenow')} wheelTaken=${event.defaultPrevented}`;
  });

  return out;
}
