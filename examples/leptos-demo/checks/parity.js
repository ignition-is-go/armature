async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const text = (id) => document.getElementById(id).textContent;
  const within = (id) => document.getElementById(id).parentElement;
  const key = (el, key, init = {}) => { el.focus(); const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); el.dispatchEvent(e); return e.defaultPrevented; };
  const ptr = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerType: 'mouse' }));
  const type = async (input, value) => { input.focus(); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await frame(); };
  const shown = (el) => getComputedStyle(el).display !== 'none';
  const who = () => document.activeElement.getAttribute('data-armature-part') || document.activeElement.tagName;
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('editable', async () => {
    const box = within('title-value');
    const preview = box.querySelector('[data-armature-part="Preview"]');
    const input = box.querySelector('input');
    const r = { start: `preview=${shown(preview)} input=${shown(input)} name=${preview.getAttribute('aria-label')}` };
    key(preview, 'F2'); await frame();
    r.f2 = `focus=${who()} field="${input.value}" preview=${shown(preview)}`;
    await type(input, 'Roadmap'); key(input, 'Escape'); await frame();
    r.escape = `${text('title-value')} focus=${who()}`;
    key(preview, 'Enter'); await frame(); await type(input, 'Roadmap'); key(input, 'Enter'); await frame();
    r.enter = `${text('title-value')} focus=${who()} input=${shown(input)}`;
    preview.click(); await frame(); await type(input, 'Plan');
    document.querySelector('[aria-label="Message"]').focus(); await frame();
    r.blurSaves = `${text('title-value')} input=${shown(input)}`;
    return r;
  });

  await step('prompt', async () => {
    const field = document.querySelector('[aria-label="Message"]');
    for (const entry of ['build', 'test']) { await type(field, entry); key(field, 'Enter'); await frame(); }
    const r = { sent: `${text('prompt-value')} field="${field.value}"` };
    await type(field, 'dep');
    key(field, 'ArrowUp'); await frame(); const first = field.value;
    key(field, 'ArrowUp'); await frame(); const second = field.value;
    key(field, 'ArrowDown'); await frame(); key(field, 'ArrowDown'); await frame();
    r.history = `${first} > ${second} > ${field.value}`;
    document.body.focus(); document.activeElement.blur?.();
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true, cancelable: true })); await frame();
    r.shortcutFocuses = document.activeElement === field;
    return r;
  });

  await step('hold', async () => {
    const button = within('hold-value').querySelector('[role=button]');
    const at = button.getBoundingClientRect();
    ptr(button, 'pointerdown', at.left + 5, at.top + 5); await wait(100);
    ptr(window, 'pointerup', at.left + 5, at.top + 5); await wait(400);
    const early = text('hold-value');
    ptr(button, 'pointerdown', at.left + 5, at.top + 5); await wait(500);
    const held = text('hold-value');
    ptr(window, 'pointerup', at.left + 5, at.top + 5); await frame();
    key(button, 'Enter'); await frame();
    return { early, held, keyboard: text('hold-value') };
  });

  await step('transfer', async () => {
    const box = within('transfer-value');
    const lists = [...box.querySelectorAll('[role=listbox]')];
    const option = (k) => box.querySelector(`[role=option][data-value="${k}"]`);
    const state = () => lists.map(l => [...l.querySelectorAll('[role=option]')].map(o => o.dataset.value).join(',')).join(' | ');
    const [add, remove] = box.querySelectorAll('[role=button]');
    const r = { start: `${state()} add=${add.getAttribute('aria-disabled')} tab=${add.getAttribute('tabindex')}` };
    option('write').click(); await frame();
    option('share').dispatchEvent(new MouseEvent('click', { bubbles: true, shiftKey: true })); await frame();
    add.click(); await frame();
    r.added = `${text('transfer-value')} | ${state()} | focus=${document.activeElement.dataset.value} | said="${document.getElementById('armature-live-polite')?.textContent}"`;
    option('share').click(); await frame(); remove.click(); await frame();
    r.removed = `${text('transfer-value')} | ${state()}`;
    const a = window.armature; const ref = a.snapshot().find(s => s.label === 'Permissions').ref;
    r.agent = JSON.stringify(a.act(ref, 'add', 'delete').value);
    return r;
  });

  await step('outline', async () => {
    const box = within('outline-value');
    const node = (k) => box.querySelector(`[role=treeitem][data-value="${k}"]`);
    const rows = () => [...box.querySelectorAll('[role=treeitem]')].map(n => `${n.dataset.value}@${n.getAttribute('aria-level')}`).join(' ');
    const r = { start: rows() };
    key(node('README.md'), 'ArrowUp', { altKey: true }); await frame();
    r.movedUp = `${rows()} | ${text('outline-value')} | focus=${document.activeElement.dataset.value}`;
    key(node('README.md'), 'ArrowRight', { altKey: true }); await frame();
    r.nested = `${rows()} | ${text('outline-value')}`;
    // Drag docs above everything, staying at the top level.
    const docs = node('docs').getBoundingClientRect(), top = box.querySelector('[role=treeitem]').getBoundingClientRect();
    ptr(node('docs'), 'pointerdown', docs.left + 10, docs.top + 5);
    ptr(window, 'pointermove', docs.left + 10, top.top - 4); await frame();
    ptr(window, 'pointerup', docs.left + 10, top.top - 4); await frame();
    r.dragged = `${rows()} | ${text('outline-value')}`;
    return r;
  });

  await step('findable', async () => {
    const a = window.armature;
    const help = () => a.snapshot().find(s => s.label === 'Support');
    const section = [...document.querySelectorAll('section')].find(s => s.querySelector('h3').textContent === 'Findable');
    const panels = [...section.querySelectorAll('[role=region]')];
    const r = { closed: panels.map(p => `${p.getAttribute('hidden')}/${getComputedStyle(p).display}`).join(' ') };
    panels[1].dispatchEvent(new Event('beforematch', { bubbles: true })); await frame();
    r.found = `open=${help().value.join(',')} hidden=${panels[1].getAttribute('hidden')}`;
    return r;
  });

  await step('theme', async () => {
    const root = document.documentElement;
    const r = { beforePaint: ['light', 'dark'].includes(root.getAttribute('data-theme')) };
    within('theme-value').querySelector('[role=radio][data-value="dark"]').click(); await frame();
    r.chosen = `${text('theme-value')} attr=${root.getAttribute('data-theme')} stored=${localStorage.getItem('armature-theme')} cookie=${document.cookie.includes('armature-theme=dark')}`;
    // The script in the page head is what applies the stored choice on the next load.
    root.removeAttribute('data-theme');
    (0, eval)(document.querySelector('head script:not([src]):not([type])').textContent);
    r.scriptApplies = root.getAttribute('data-theme');
    within('theme-value').querySelector('[role=radio][data-value="system"]').click(); await frame();
    return r;
  });

  await step('layers', async () => {
    const trigger = document.querySelector('[data-armature-part="Trigger"][aria-label="Share"]');
    const content = document.getElementById(trigger.getAttribute('aria-controls'));
    const a = window.armature; const dialog = a.snapshot().find(s => s.label === 'Confirm').ref;
    trigger.click(); await frame();
    a.act(dialog, 'open'); await frame();
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); await wait(400);
    const underDialog = shown(content);
    a.act(dialog, 'close'); await frame();
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); await wait(400);
    return { staysOpenUnderADialog: underDialog, closesOnceTheDialogIsGone: !shown(content) };
  });

  return out;
}
