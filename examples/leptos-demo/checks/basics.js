async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 40))));
  const text = (id) => document.getElementById(id).textContent;
  const key = (el, key, init = {}) => { el.focus(); const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); el.dispatchEvent(e); return e.defaultPrevented; };
  const ptr = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerId: 1 }));
  const byLabel = (label) => document.querySelector(`[aria-label="${label}"]`);
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('slider', async () => {
    const thumb = document.querySelector('[role=slider]');
    const track = document.querySelector('[data-armature-part="Track"]');
    const t = track.getBoundingClientRect();
    const r = { aria: `${thumb.getAttribute('aria-valuenow')}/${thumb.getAttribute('aria-valuemin')}-${thumb.getAttribute('aria-valuemax')} label=${thumb.getAttribute('aria-label')}` };
    ptr(track, 'pointerdown', t.left + t.width * 0.5, t.top + 2); await frame();
    r.press = text('volume-value');
    ptr(window, 'pointermove', t.left + t.width * 0.8, t.top + 300); await frame();
    r.dragOutsideTrack = text('volume-value');
    ptr(window, 'pointerup', t.left + t.width * 0.8, t.top + 300); await frame();
    ptr(window, 'pointermove', t.left, t.top); await frame();
    r.afterRelease = text('volume-value');
    r.focusOnThumb = document.activeElement === thumb;
    key(thumb, 'ArrowLeft'); key(thumb, 'PageUp'); await frame();
    r.keys = `${text('volume-value')} aria=${thumb.getAttribute('aria-valuenow')}`;
    return r;
  });
  await step('switch', async () => {
    const sw = document.querySelector('[role=switch]');
    sw.click(); await frame();
    const on = `${text('wifi-value')} checked=${sw.getAttribute('aria-checked')}`;
    key(sw, ' '); await frame();
    return `${on} -> ${text('wifi-value')}`;
  });
  await step('accordion', async () => {
    const triggers = [...document.querySelectorAll('[role=button][aria-expanded]')];
    const regions = [...document.querySelectorAll('[role=region]')];
    const shown = () => regions.map(r => getComputedStyle(r).display).join(',');
    const start = shown();
    triggers[0].click(); await frame(); triggers[1].click(); await frame();
    const after = `${text('faq-value')} | ${shown()} | ${triggers.map(b => b.getAttribute('aria-expanded')).join(',')}`;
    key(triggers[1], 'ArrowUp'); await frame();
    return { start, after, arrowMovesFocus: document.activeElement === triggers[0] };
  });
  await step('rating', async () => {
    const stars = [...document.querySelectorAll('[role=radio]')];
    stars[2].click(); await frame();
    const click = `${text('rating-value')} filled=${stars.filter(s => s.hasAttribute('data-active')).length} checked=${stars.map(s => s.getAttribute('aria-checked')).join(',')}`;
    key(stars[2], 'ArrowRight'); await frame();
    return { click, arrow: `${text('rating-value')} focus=${document.activeElement.dataset.value}` };
  });
  await step('tree', async () => {
    const node = (k) => document.querySelector(`[role=treeitem][data-value="${k}"]`);
    const shown = () => [...document.querySelectorAll('[role=treeitem]')].filter(n => getComputedStyle(n).display !== 'none').map(n => n.dataset.value).join(',');
    const start = shown();
    key(node('src'), 'ArrowRight'); await frame();
    key(node('src'), 'ArrowRight'); await frame();
    const open = `focus=${document.activeElement.dataset.value} level=${document.activeElement.getAttribute('aria-level')} shown=${shown()}`;
    key(document.activeElement, 'Enter'); await frame();
    return { start, open, select: text('file-value') };
  });
  await step('command', async () => {
    const input = byLabel('Commands');
    input.focus(); input.value = 'file'; input.dispatchEvent(new Event('input', { bubbles: true })); await frame();
    const opts = [...document.querySelectorAll('[data-armature-part="Option"]')].filter(o => ['open','save','close'].includes(o.dataset.value));
    const filter = opts.map(o => `${o.dataset.value}:${getComputedStyle(o).display === 'none' ? 'hidden' : 'shown'}${o.hasAttribute('data-active') ? '*' : ''}`).join(' ');
    key(input, 'ArrowDown'); await frame(); key(input, 'Enter'); await frame();
    return { filter, chose: `${text('command-value')} inputKept="${input.value}" focusOnInput=${document.activeElement === input}` };
  });
  await step('number', async () => {
    const qty = document.querySelector('input[aria-label="Quantity"]');
    const plus = document.querySelector('[data-armature-part="Increment"]');
    const start = `${qty.value} role=${qty.getAttribute('role')} now=${qty.getAttribute('aria-valuenow')}`;
    qty.focus(); qty.value = '7'; qty.dispatchEvent(new Event('input', { bubbles: true })); await frame();
    key(qty, 'Enter'); await frame();
    const typed = text('quantity-value');
    ptr(plus, 'pointerdown', 0, 0);
    await new Promise(r => setTimeout(r, 700));
    ptr(window, 'pointerup', 0, 0); await frame();
    const held = Number(text('quantity-value').replace('Quantity: ', ''));
    await new Promise(r => setTimeout(r, 300));
    return { start, typed, heldFor700ms: `${held} (${held - 7} steps)`, stoppedAfterRelease: Number(text('quantity-value').replace('Quantity: ', '')) === held, field: qty.value };
  });
  await step('sortable', async () => {
    const item = (k) => document.querySelector(`[role=listitem][data-value="${k}"]`);
    const order = () => [...document.querySelectorAll('[role=listitem]')].map(n => n.dataset.value).join(',');
    const live = () => document.getElementById('armature-live-polite')?.textContent;
    const m = item('milk').getBoundingClientRect(), b = item('bread').getBoundingClientRect();
    ptr(item('milk'), 'pointerdown', m.left + 10, m.top + 5);
    ptr(window, 'pointermove', m.left + 10, m.top + 12); await frame();
    ptr(window, 'pointermove', b.left + 10, b.bottom - 2); await frame();
    const dragging = item('milk').getAttribute('aria-selected');
    ptr(window, 'pointerup', b.left + 10, b.bottom - 2); await frame(); await frame();
    const drag = `${text('groceries-value')} | dom=${order()} | said="${live()}"`;
    key(item('jam'), ' '); await frame(); key(item('jam'), 'ArrowUp'); await frame(); key(item('jam'), ' '); await frame(); await frame();
    return { dragging, drag, keyboard: `dom=${order()} | said="${live()}"` };
  });
  out.labels = [...document.querySelectorAll('[aria-label]')].map(e => `${e.getAttribute('role') || e.tagName.toLowerCase()}=${e.getAttribute('aria-label')}`).join(' | ');
  out.agent = window.armature.snapshot().map(s => `${s.name}:${s.label}=[${s.value}]`).join(' ');
  return out;
}
