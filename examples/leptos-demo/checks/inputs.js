async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const text = (id) => document.getElementById(id).textContent;
  const key = (el, key, init = {}) => { el.focus(); const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); el.dispatchEvent(e); return e.defaultPrevented; };
  const keyHere = (key, init = {}) => { const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); document.activeElement.dispatchEvent(e); return e.defaultPrevented; };
  const ptr = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0 }));
  const type = async (input, value) => { input.focus(); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await frame(); };
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('calendar', async () => {
    const day = (iso) => document.querySelector(`[role=gridcell][data-value="${iso}"]`);
    const r = { cells: document.querySelectorAll('[role=gridcell]').length, stop: document.querySelector('[role=gridcell][tabindex="0"]').dataset.value };
    key(day('2026-11-17'), 'ArrowRight'); await frame();
    r.arrow = document.activeElement.dataset.value;
    keyHere('ArrowDown'); await frame(); keyHere('ArrowDown'); await frame();
    r.crossedIntoDecember = `${text('calendar-month')} focus=${document.activeElement.dataset.value}`;
    keyHere('Enter'); await frame();
    r.chosen = `${text('day-value')} selected=${document.activeElement.getAttribute('aria-selected')}`;
    const next = document.querySelector('[data-armature-part="NextMonth"]');
    key(next, 'Enter'); await frame();
    r.enterOnNextMonth = `${text('calendar-month')} ${text('day-value')} label=${next.getAttribute('aria-label')}`;
    return r;
  });
  await step('toolbar', async () => {
    const b = (k) => document.querySelector(`[role=toolbar] [data-value="${k}"]`);
    key(b('bold'), 'ArrowRight'); await frame();
    const moved = document.activeElement.dataset.value;
    keyHere('Enter'); await frame();
    return { moved, pressed: text('pressed-value'), stops: document.querySelectorAll('[role=toolbar] [tabindex="0"]').length };
  });
  await step('otp', async () => {
    const input = document.querySelector('input[aria-label="Code"]');
    await type(input, '12-34 56');
    return { code: text('code-value'), cells: [...document.querySelectorAll('.otp-cells span')].map(s => s.textContent).join('|'), field: input.value };
  });
  await step('tokens', async () => {
    const input = document.querySelector('input[aria-label="Tags"]');
    await type(input, 'rust, wasm, ');
    const afterPaste = `${text('tags-value')} field="${input.value}"`;
    await type(input, 'gpui'); key(input, 'Enter'); await frame();
    const afterEnter = text('tags-value');
    key(input, 'Backspace'); await frame();
    const afterBackspace = text('tags-value');
    const token = document.querySelector('[data-armature-part="Token"][data-value="rust"]');
    const name = token.getAttribute('aria-label');
    token.click(); await frame();
    return { afterPaste, afterEnter, afterBackspace, tokenName: name, afterRemove: text('tags-value'), focusOnField: document.activeElement === input };
  });
  await step('resizable', async () => {
    const handle = document.querySelector('[role=separator]');
    const box = handle.parentElement.getBoundingClientRect();
    const r = { aria: `${handle.getAttribute('aria-valuenow')} ${handle.getAttribute('aria-orientation')}` };
    ptr(handle, 'pointerdown', box.left + box.width / 2, box.top + 5);
    ptr(window, 'pointermove', box.left + box.width * 0.25, box.top + 5); await frame();
    ptr(window, 'pointerup', box.left + box.width * 0.25, box.top + 5); await frame();
    r.dragged = `${text('split-value')} panel=${Math.round(document.querySelector('.panel').getBoundingClientRect().width / box.width * 100)}%`;
    key(handle, 'End'); await frame();
    r.end = text('split-value');
    return r;
  });
  await step('relations', async () => {
    const tab = document.querySelector('[role=tab][data-value="fruit"]');
    const panel = document.getElementById(tab.getAttribute('aria-controls'));
    const header = document.querySelector('[role=button][aria-expanded][data-value="shipping"]');
    const region = document.getElementById(header.getAttribute('aria-controls'));
    return { tabControls: panel?.getAttribute('role'), panelLabelledByTab: panel?.getAttribute('aria-labelledby') === tab.id, headerControls: region?.getAttribute('role'), regionLabelledBy: region?.getAttribute('aria-labelledby') === header.id };
  });
  // Behaviors taken from upstream's tests, seen through the real page.
  await step('upstream', async () => {
    const a = window.armature;
    const of = (label) => a.snapshot().find(s => s.label === label);
    const r = {};
    // Select: an agent can clear the choice.
    a.act(of('Size').ref, 'select', 'small'); await frame();
    const chosen = of('Size').value.join(',');
    a.act(of('Size').ref, 'clear'); await frame();
    r.selectClear = `${chosen} -> [${of('Size').value.join(',')}]`;
    // Number input: End and Home go to the ends of the range.
    const before = of('Amount').value.join(',');
    const amount = [...document.querySelectorAll('input')].find(el => el.getAttribute('aria-label') === 'Amount' || el.closest('[aria-label="Amount"]'));
    key(amount, 'End'); await frame();
    const atEnd = of('Amount').value.join(',');
    key(amount, 'Home'); await frame();
    r.numberEnds = `end=${atEnd} home=${of('Amount').value.join(',')}`;
    a.act(of('Amount').ref, 'set', before); await frame();
    // Combobox: deleting the text clears the choice, and Escape brings it back.
    const fruit = [...document.querySelectorAll('input')].find(el => el.getAttribute('aria-label') === 'Favourite fruit' || el.closest('[aria-label="Favourite fruit"]'));
    a.act(of('Favourite fruit').ref, 'select', 'apricot'); await frame();
    const picked = `${of('Favourite fruit').value.join(',')}/${fruit.value}`;
    await type(fruit, '');
    const deleted = `[${of('Favourite fruit').value.join(',')}]`;
    key(fruit, 'Escape'); await frame();
    r.comboboxEscape = `${picked} -> ${deleted} -> ${of('Favourite fruit').value.join(',')}/${fruit.value}`;
    a.act(of('Favourite fruit').ref, 'clear'); await frame();
    return r;
  });
  return out;
}