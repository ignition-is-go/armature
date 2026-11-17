async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const text = (id) => document.getElementById(id).textContent;
  const within = (id) => document.getElementById(id).parentElement;
  const section = (title) => [...document.querySelectorAll('section')].find(s => s.querySelector('h3').textContent === title);
  const key = (el, key, init = {}) => { el.focus(); const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); el.dispatchEvent(e); return e.defaultPrevented; };
  const keyHere = (key, init = {}) => { const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); document.activeElement.dispatchEvent(e); return e.defaultPrevented; };
  const ptr = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerType: 'mouse' }));
  const type = async (input, value) => { input.focus(); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await frame(); };
  const shown = (el) => !!el && getComputedStyle(el).display !== 'none';
  const who = () => document.activeElement.dataset.value || document.activeElement.tagName;
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('eventList', async () => {
    const events = [...section('Event list').querySelectorAll('[role=listitem]')];
    const current = events.find(e => e.getAttribute('aria-current'))?.dataset.value;
    key(events.find(e => e.tabIndex === 0), 'ArrowDown'); await frame();
    return `current=${current} tabStop=${events.find(e => e.tabIndex === 0)?.dataset.value} focus=${who()}`;
  });

  await step('rangeSlider', async () => {
    const box = within('price-value');
    const track = box.querySelector('[data-armature-part="Track"]');
    const at = track.getBoundingClientRect();
    ptr(track, 'pointerdown', at.left + at.width * 0.1, at.top + 2); ptr(window, 'pointerup', at.left, at.top); await frame();
    const pressed = text('price-value');
    const high = box.querySelectorAll('[role=slider]')[1];
    key(high, 'Home'); await frame();
    return { pressed, homeStopsAtTheOtherGrip: text('price-value'), max: box.querySelectorAll('[role=slider]')[0].getAttribute('aria-valuemax') };
  });

  await step('panelGroup', async () => {
    const box = within('panes-value');
    const handle = box.querySelector('[role=separator]');
    key(handle, 'ArrowRight', { shiftKey: true }); await frame();
    const keyed = text('panes-value');
    const at = handle.getBoundingClientRect(); const width = box.querySelector('.panes').getBoundingClientRect().width;
    ptr(handle, 'pointerdown', at.left + 2, at.top + 5); ptr(window, 'pointermove', at.left + 2 - width, at.top + 5); await frame(); ptr(window, 'pointerup', 0, 0); await frame();
    return { keyed, draggedToTheMinimum: text('panes-value') };
  });

  await step('rating', async () => {
    const box = within('stars-value');
    const star = box.querySelector('[data-armature-part="Star"][data-value="3"]');
    const at = star.getBoundingClientRect();
    ptr(star, 'pointerdown', at.left + at.width * 0.25, at.top + 4); ptr(window, 'pointerup', 0, 0); await frame();
    const half = text('stars-value');
    ptr(star, 'pointerdown', at.left + at.width * 0.75, at.top + 4); ptr(window, 'pointerup', 0, 0); await frame();
    keyHere('ArrowLeft'); await frame();
    return { half, wholeThenBackAHalf: text('stars-value'), value: box.querySelector('[role=slider]').getAttribute('aria-valuenow') };
  });

  await step('dateSpan', async () => {
    const box = within('stay-value');
    const day = (iso) => box.querySelector(`[role=gridcell][data-value="${iso}"]`);
    const months = box.querySelectorAll('.month').length;
    day('2026-12-03').click(); await frame(); day('2026-11-28').click(); await frame();
    const span = `${text('stay-value')} between=${day('2026-11-30').getAttribute('aria-selected')} outside=${day('2026-12-04').getAttribute('aria-selected')}`;
    box.querySelector('[aria-label="Later"]').click(); await frame();
    return { months, span, paged: [...box.querySelectorAll('.month .hint')].map(h => h.textContent).join(' ') };
  });

  await step('multipleSelect', async () => {
    const box = within('sizes-value');
    const trigger = box.querySelector('[data-armature-part="Trigger"]');
    trigger.click(); await frame();
    for (const size of ['small', 'large']) { box.querySelector(`[role=option][data-value="${size}"]`).click(); await frame(); }
    return `${text('sizes-value')} open=${trigger.getAttribute('aria-expanded')}`;
  });

  await step('lazyTree', async () => {
    const box = within('remote-value');
    const rows = () => [...box.querySelectorAll('[role=treeitem]')].map(r => r.dataset.value).join(' ');
    const before = `${rows()} expandable=${box.querySelector('[role=treeitem]').getAttribute('aria-expanded')}`;
    key(box.querySelector('[role=treeitem]'), 'ArrowRight'); await frame(); await frame();
    return { before, after: `${text('remote-value')} | ${rows()}` };
  });

  await step('commandPages', async () => {
    const box = within('palette-value');
    const input = box.querySelector('input');
    const options = () => [...box.querySelectorAll('[role=option]')].filter(shown).map(o => o.dataset.value).join(',');
    const start = options();
    await type(input, 'theme'); key(input, 'Enter'); await frame();
    const page = `${options()} field="${input.value}" ran="${text('palette-value')}"`;
    key(input, 'ArrowDown'); await frame(); key(input, 'Enter'); await frame();
    const ran = text('palette-value');
    key(input, 'Backspace'); await frame();
    return { start, page, ran, back: options() };
  });

  return out;
}
