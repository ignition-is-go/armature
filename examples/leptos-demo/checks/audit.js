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

  await step('numberScrubAndWheel', async () => {
    const box = within('amount-value');
    const scrub = box.querySelector('[data-armature-part="Scrub"]'); const input = box.querySelector('input');
    const at = scrub.getBoundingClientRect();
    ptr(scrub, 'pointerdown', at.left + 5, at.top + 5); ptr(window, 'pointermove', at.left + 45, at.top + 5); await frame(); ptr(window, 'pointerup', 0, 0); await frame();
    const scrubbed = text('amount-value');
    const wheel = () => { const e = new WheelEvent('wheel', { deltaY: -100, bubbles: true, cancelable: true }); input.dispatchEvent(e); return e.defaultPrevented; };
    const unfocused = wheel(); await frame();
    input.focus(); await frame(); const focused = wheel(); await frame();
    return { scrubbed, wheelIgnoredWithoutFocus: !unfocused, wheelTakenWithFocus: focused, after: text('amount-value') };
  });

  await step('twelveHour', async () => {
    const box = within('time-value');
    const fields = () => [...box.querySelectorAll('input')].map(i => i.value).join(' ');
    box.querySelector('input').focus(); await frame();
    for (const digit of '202611020230') { keyHere(digit); await frame(); }
    const morning = `${text('time-value')} | ${fields()} | focus=${document.activeElement.getAttribute('aria-label')}`;
    keyHere('p'); await frame();
    return { morning, afternoon: `${text('time-value')} | ${fields()}` };
  });

  await step('completionAndMore', async () => {
    const box = within('city-value');
    const input = box.querySelector('input');
    await type(input, 'li');
    const suggested = document.getElementById('city-completion').textContent;
    key(input, 'ArrowDown'); await frame(); key(input, 'ArrowDown'); await frame();
    const askedForMore = text('city-value');
    await type(input, 'os'); key(input, 'Tab'); await frame();
    return { suggested, askedForMore, tabTakesIt: `${text('city-value')} field="${input.value}"` };
  });

  await step('commandArguments', async () => {
    const box = within('line-value');
    const input = box.querySelector('input');
    const hint = () => document.getElementById('command-hint').textContent;
    await type(input, '/dep'); key(input, 'Enter'); await frame();
    const first = `"${input.value}" needs ${hint()}`;
    await type(input, '/deploy prod ');
    const second = hint();
    await type(input, '/deploy prod 1.2 ');
    return { first, second, done: hint() === '' };
  });

  await step('cropZoom', async () => {
    const box = within('zoomed-value');
    const area = box.querySelector('[data-armature-part="Area"]');
    const crop = box.querySelector('.crop-box');
    const before = `${text('zoom-value')} width=${crop.style.width}`;
    for (let i = 0; i < 4; i++) { area.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, bubbles: true, cancelable: true })); await frame(); }
    return { before, after: `${text('zoom-value')} selectionLooksLarger=${parseFloat(crop.style.width) > 75}` };
  });

  await step('locale', async () => document.getElementById('locale-value').textContent.replace(/\s+/g, ' ').trim());

  return out;
}
