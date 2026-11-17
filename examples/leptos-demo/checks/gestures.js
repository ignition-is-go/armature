async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const text = (id) => document.getElementById(id).textContent;
  const q = (s) => document.querySelector(s);
  const part = (name, extra = '') => q(`[data-armature-part="${name}"]${extra}`);
  const key = (el, key, init = {}) => { el.focus(); const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); el.dispatchEvent(e); return e.defaultPrevented; };
  const ptr = (target, type, x, y, init = {}) => target.dispatchEvent(new PointerEvent(type, { bubbles: type !== 'pointerenter' && type !== 'pointerleave', cancelable: true, clientX: x, clientY: y, button: 0, pointerType: 'mouse', ...init }));
  const shown = (el) => getComputedStyle(el).display !== 'none';
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('popover', async () => {
    const trigger = q('[data-armature-part="Trigger"][aria-label="Share"]');
    const content = document.getElementById(trigger.getAttribute('aria-controls'));
    trigger.scrollIntoView(); await frame();
    trigger.click(); await frame();
    const t = trigger.getBoundingClientRect(), c = content.getBoundingClientRect();
    const arrow = content.querySelector('[data-armature-part="Arrow"]').getBoundingClientRect();
    const r = {
      opened: shown(content), gap: Math.round(c.top - t.bottom),
      labelledBy: document.getElementById(content.getAttribute('aria-labelledby'))?.textContent,
      describedBy: document.getElementById(content.getAttribute('aria-describedby'))?.textContent,
      arrowPointsAtTrigger: Math.abs((arrow.left + arrow.width / 2) - (t.left + t.width / 2)) < 2,
    };
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));  await frame();
    r.closedByOutsidePress = !shown(content);
    return r;
  });
  await step('tooltip', async () => {
    const trigger = q('[data-armature-part="Trigger"][aria-describedby]');
    const tip = q('[role=tooltip]');
    trigger.scrollIntoView(); await frame();
    const r = { describedBy: trigger.getAttribute('aria-describedby') === tip.id, hiddenAtFirst: !shown(tip) };
    ptr(trigger, 'pointerenter', 0, 0); await wait(200);
    r.stillHiddenAt200ms = !shown(tip);
    await wait(450);
    r.shownAfterDelay = shown(tip);
    ptr(trigger, 'pointerleave', 0, 0); await frame();
    r.hiddenOnLeave = !shown(tip);
    ptr(trigger, 'pointerenter', 0, 0, { pointerType: 'touch' }); await wait(650);
    r.touchNeverHovers = !shown(tip);
    trigger.focus(); await frame();
    r.shownOnFocus = shown(tip);
    key(trigger, 'Escape'); await frame();
    r.hiddenOnEscape = !shown(tip);
    return r;
  });
  await step('color', async () => {
    const area = part('Area'), hue = part('Hue');
    area.scrollIntoView(); await frame();
    const a = area.getBoundingClientRect(), h = hue.getBoundingClientRect();
    ptr(hue, 'pointerdown', h.left + h.width / 3, h.top + 3); await frame();
    ptr(window, 'pointerup', 0, 0); await frame();
    const afterHue = text('color-value');
    ptr(area, 'pointerdown', a.right, a.top); await frame();
    ptr(window, 'pointermove', a.left, a.top); await frame();
    const white = text('color-value');
    ptr(window, 'pointermove', a.right, a.top); await frame();
    ptr(window, 'pointerup', a.right, a.top); await frame();
    const input = q('input[aria-label="Colour"]');
    const r = { afterHue, white, pureHueAgain: text('color-value'), field: input.value, swatch: getComputedStyle(document.getElementById('swatch')).backgroundColor };
    input.focus(); input.value = '#ff8800'; input.dispatchEvent(new Event('input', { bubbles: true })); await frame();
    key(input, 'Enter'); await frame();
    r.typed = text('color-value');
    return r;
  });
  await step('swipe', async () => {
    const row = part('Row');
    row.scrollIntoView(); await frame();
    const b = row.getBoundingClientRect();
    const actions = [...document.querySelectorAll('[data-armature-part="Action"]')];
    const r = { hiddenAtFirst: actions.every(a => !shown(a)) };
    const swipe = async (distance) => {
      ptr(row, 'pointerdown', b.left + 200, b.top + 5);
      ptr(window, 'pointermove', b.left + 200 - distance / 2, b.top + 5); await wait(250);
      ptr(window, 'pointermove', b.left + 200 - distance, b.top + 5); await wait(250);
      ptr(window, 'pointerup', b.left + 200 - distance, b.top + 5); await frame();
    };
    await swipe(50);
    r.revealed = `${actions.every(shown)} offset=${q('.swipe-slide').style.transform}`;
    actions[1].click(); await frame();
    r.pressed = `${text('swipe-value')} closed=${q('.swipe-slide').style.transform}`;
    await swipe(220);
    r.fullSwipe = text('swipe-value');
    return r;
  });
  await step('virtual', async () => {
    const viewport = part('Viewport');
    viewport.scrollIntoView(); await frame();
    const rendered = () => document.querySelectorAll('.virtual-row').length;
    const r = { rendered: rendered(), scrollHeight: viewport.scrollHeight };
    viewport.scrollTop = 120000; viewport.dispatchEvent(new Event('scroll')); await frame();
    const first = q('.virtual-row [role=option]');
    r.afterScroll = `${rendered()} rows, first=${first.dataset.value}`;
    const row5000 = q('[role=option][data-value="5000"]');
    row5000.click(); await frame();
    key(row5000, 'ArrowDown'); await frame(); key(document.activeElement, 'ArrowDown'); await frame();
    key(document.activeElement, 'ArrowDown'); await frame(); key(document.activeElement, 'ArrowDown'); await frame(); key(document.activeElement, 'ArrowDown'); await frame();
    key(document.activeElement, 'Enter'); await frame();
    r.keyboard = `${text('row-value')} focus=${document.activeElement.dataset.value} scrollTop=${viewport.scrollTop}`;
    return r;
  });
  await step('timeline', async () => {
    const track = q('[data-armature-part="Track"][role="list"]');
    track.scrollIntoView(); await frame();
    const t = track.getBoundingClientRect();
    const hour = t.width / 24;
    ptr(track, 'pointerdown', t.left + hour * 9.1, t.top + 5);
    ptr(window, 'pointermove', t.left + hour * 12.9, t.top + 5); await frame();
    ptr(window, 'pointerup', t.left + hour * 12.9, t.top + 5); await frame();
    const drawn = text('plan-value');
    const span = q('[data-armature-part="Span"]');
    const s = span.getBoundingClientRect();
    ptr(span, 'pointerdown', s.left + 5, s.top + 5);
    ptr(window, 'pointermove', s.left + 5 + hour * 2, s.top + 5); await frame();
    ptr(window, 'pointerup', s.left + 5 + hour * 2, s.top + 5); await frame();
    const moved = text('plan-value');
    key(q('[data-armature-part="Span"]'), 'ArrowRight', { shiftKey: true }); await frame();
    return { drawn, moved, resizedByKeyboard: text('plan-value'), said: document.getElementById('armature-live-polite')?.textContent };
  });
  await step('grid', async () => {
    const cell = (k) => q(`[role=gridcell][data-value="${k}"]`);
    cell('0,0').scrollIntoView(); await frame();
    cell('0,0').click(); await frame();
    key(document.activeElement, 'ArrowDown', { shiftKey: true }); await frame();
    key(document.activeElement, 'ArrowRight', { shiftKey: true }); await frame();
    const selected = text('cells-value');
    let copied = null;
    const original = navigator.clipboard.writeText.bind(navigator.clipboard);
    navigator.clipboard.writeText = (t) => { copied = t; return Promise.resolve(); };
    key(document.activeElement, 'c', { ctrlKey: true }); await frame();
    navigator.clipboard.writeText = original;
    const data = new DataTransfer(); data.setData('text/plain', 'X\tY\nZ\tW');
    cell('1,1').click(); await frame();
    document.activeElement.dispatchEvent(new ClipboardEvent('paste', { clipboardData: data, bubbles: true, cancelable: true })); await frame();
    return { selected, copied: JSON.stringify(copied), pasted: [...document.querySelectorAll('[role=grid][aria-multiselectable] [role=gridcell]')].map(c => c.textContent).join(','), edits: text('cells-value') };
  });
  await step('canvas', async () => {
    const surface = part('Surface');
    surface.scrollIntoView(); await frame();
    const s = surface.getBoundingClientRect();
    const node = (k) => q(`[data-armature-part="Node"][data-value="${k}"]`);
    ptr(surface, 'pointerdown', s.left + 2, s.top + 2);
    ptr(window, 'pointermove', s.left + 260, s.top + 70); await frame();
    const lasso = `${text('canvas-selected')} lassoShown=${shown(q('.lasso'))}`;
    ptr(window, 'pointerup', s.left + 260, s.top + 70); await frame();
    const a = node('a').getBoundingClientRect();
    ptr(node('a'), 'pointerdown', a.left + 20, a.top + 20);
    ptr(window, 'pointermove', a.left + 50, a.top + 60); await frame();
    ptr(window, 'pointerup', a.left + 50, a.top + 60); await frame();
    const moved = text('layout-value');
    key(surface, 'z', { ctrlKey: true }); await frame();
    const undone = text('layout-value');
    q('[data-armature-part="PortOut"][data-value="a"]').click(); await frame();
    q('[data-armature-part="PortIn"][data-value="c"]').click(); await frame();
    return { lasso, moved, undone, links: text('links'), said: document.getElementById('armature-live-polite')?.textContent };
  });
  return out;
}
