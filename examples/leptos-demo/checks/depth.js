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

  return out;
}
