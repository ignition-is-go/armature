async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const text = (id) => document.getElementById(id).textContent;
  const within = (id) => document.getElementById(id).parentElement;
  const section = (title) => [...document.querySelectorAll('section')].find(s => s.querySelector('h3').textContent === title);
  const key = (el, key, init = {}) => { el.focus(); const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); el.dispatchEvent(e); return e.defaultPrevented; };
  const keyHere = (key, init = {}) => { const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); document.activeElement.dispatchEvent(e); return e.defaultPrevented; };
  const ptr = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: x, clientY: y, button: 0, pointerType: 'mouse' }));
  const type = async (input, value) => { input.focus(); input.value = value; input.dispatchEvent(new Event('input', { bubbles: true })); await frame(); };
  const shown = (el) => !!el && getComputedStyle(el).display !== 'none';
  const who = () => document.activeElement.dataset.value || document.activeElement.getAttribute('data-armature-part') || document.activeElement.tagName;
  const step = async (name, fn) => { try { out[name] = await fn(); } catch (e) { out[name] = 'ERROR ' + e.message; } };

  await step('button', async () => {
    const button = within('button-value').querySelector('[role=button]');
    button.click(); await frame(); key(button, 'Enter'); await frame();
    return text('button-value');
  });

  await step('checkbox', async () => {
    const box = within('agreed-value').querySelector('[role=checkbox]');
    key(box, ' '); await frame();
    const single = `${text('agreed-value')} checked=${box.getAttribute('aria-checked')} enterLeftAlone=${!key(box, 'Enter')}`;
    const [all, email, sms] = [...within('alerts-value').querySelectorAll('[role=checkbox]')].slice(1);
    sms.click(); await frame();
    const some = `${text('alerts-value')} all=${all.getAttribute('aria-checked')}`;
    all.click(); await frame();
    return { single, some, everything: `${text('alerts-value')} all=${all.getAttribute('aria-checked')} email=${email.getAttribute('aria-checked')}` };
  });

  await step('toggles', async () => {
    const toggles = [...within('align-value').querySelectorAll('[role=button]')];
    toggles[0].click(); await frame(); key(toggles[0], 'ArrowRight'); await frame();
    const moved = `${text('align-value')} focus=${who()} pressed=${toggles.map(t => t.getAttribute('aria-pressed')).join(',')}`;
    keyHere(' '); await frame();
    return { moved, pressed: `${text('align-value')} tabStops=${toggles.filter(t => t.tabIndex === 0).length}` };
  });

  await step('pagination', async () => {
    const box = within('page-value');
    const row = () => [...box.querySelectorAll('.pages > *')].map(n => n.textContent).join(' ');
    const r = { start: `${row()} prev=${box.querySelector('[aria-label="Previous page"]').getAttribute('aria-disabled')}` };
    box.querySelector('[data-value="5"]').click(); await frame();
    r.five = `${text('page-value')} | ${row()} | current=${box.querySelector('[aria-current]').dataset.value}`;
    box.querySelector('[aria-label="Next page"]').click(); await frame();
    r.next = text('page-value');
    return r;
  });

  await step('textField', async () => {
    const box = within('email-value');
    const [input, password] = box.querySelectorAll('input');
    const error = box.querySelector('[role=alert]');
    await type(input, 'ada');
    const typing = `error=${shown(error)} invalid=${input.getAttribute('aria-invalid')}`;
    password.focus(); await frame();
    const left = `error=${shown(error)} invalid=${input.getAttribute('aria-invalid')} labelled=${document.getElementById(input.getAttribute('aria-labelledby'))?.textContent}`;
    await type(input, 'ada@example.com');
    const reveal = box.querySelector('[aria-label="Show password"]');
    const hidden = password.type; reveal.click(); await frame();
    return { typing, left, fixed: `${text('email-value')} error=${shown(error)}`, password: `${hidden} > ${password.type} pressed=${reveal.getAttribute('aria-pressed')}` };
  });

  await step('avatar', async () => {
    const box = section('Avatar');
    await wait(100);
    return `image=${shown(box.querySelector('[data-armature-part="Image"]'))} fallback=${shown(box.querySelector('[data-armature-part="Fallback"]'))}`;
  });

  await step('menu', async () => {
    const box = within('menu-value');
    const trigger = box.querySelector('[data-armature-part="Trigger"]');
    const items = () => [...box.querySelectorAll('[role^=menuitem]')].filter(shown).map(i => i.dataset.value).join(',');
    key(trigger, 'ArrowDown'); await frame();
    const r = { opened: `items=${items()} focus=${who()} expanded=${trigger.getAttribute('aria-expanded')}` };
    keyHere('ArrowDown'); await frame(); keyHere(' '); await frame();
    r.checkbox = `${text('menu-value')} open=${trigger.getAttribute('aria-expanded')}`;
    keyHere('ArrowDown'); await frame(); keyHere('ArrowRight'); await frame();
    r.submenu = `items=${items()} focus=${who()}`;
    keyHere('Escape'); await frame();
    r.back = `items=${items()} focus=${who()}`;
    keyHere('Home'); await frame(); keyHere('Enter'); await frame();
    r.ran = `${text('menu-value')} open=${trigger.getAttribute('aria-expanded')} focus=${who()}`;
    return r;
  });

  await step('menubar', async () => {
    const box = within('bar-value');
    const [file, view] = box.querySelectorAll('[data-armature-part="Trigger"]');
    key(file, 'ArrowDown'); await frame();
    const opened = `focus=${who()} file=${file.getAttribute('aria-expanded')}`;
    keyHere('ArrowRight'); await frame();
    const across = `focus=${who()} view=${view.getAttribute('aria-expanded')} file=${file.getAttribute('aria-expanded')}`;
    const content = document.getElementById(view.getAttribute('aria-controls'));
    const under = Math.round(content.getBoundingClientRect().left - view.getBoundingClientRect().left);
    keyHere('Enter'); await frame();
    return { opened, across, popupUnderItsOwnTitle: under === 0, ran: `${text('bar-value')} focus=${who()}` };
  });

  await step('combobox', async () => {
    const box = within('combo-value');
    const input = box.querySelector('input');
    const options = () => [...box.querySelectorAll('[role=option]')].filter(shown).map(o => o.dataset.value).join(',');
    await type(input, 'ap');
    const r = { narrowed: `${options()} active=${box.querySelector('[role=option][data-active]')?.dataset.value} said="${document.getElementById('armature-live-polite')?.textContent}"` };
    key(input, 'ArrowDown'); await frame(); key(input, 'Enter'); await frame();
    r.chosen = `${text('combo-value')} field="${input.value}" open=${input.getAttribute('aria-expanded')}`;
    await type(input, 'zzz');
    r.nothing = `empty=${shown(box.querySelector('[data-armature-part="Empty"]'))}`;
    document.body.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })); await frame();
    r.reverts = `field="${input.value}"`;
    return r;
  });

  await step('navigation', async () => {
    const box = within('nav-value');
    const trigger = box.querySelector('[data-armature-part="Trigger"]');
    const panel = box.querySelector('[data-armature-part="Panel"]');
    trigger.click(); await frame();
    const opened = `${text('nav-value')} panel=${shown(panel)}`;
    key(trigger, 'ArrowRight'); await frame();
    const link = `focus=${who()} current=${document.activeElement.getAttribute('aria-current')}`;
    key(trigger, 'Escape'); await frame();
    return { opened, link, closed: `panel=${shown(panel)}` };
  });

  await step('toasts', async () => {
    const box = within('toast-value');
    const notify = box.querySelector('[role=button]');
    notify.click(); await frame(); await wait(150);
    const toast = box.querySelector('[data-armature-part="Toast"]');
    const r = { shown: `${text('toast-value')} title=${toast?.querySelector('[data-armature-part="Title"]')?.textContent} said="${document.getElementById('armature-live-polite')?.textContent}"` };
    await wait(1800);
    r.timedOut = text('toast-value');
    notify.click(); await frame(); await wait(100);
    box.querySelector('[aria-label="Dismiss"]').click(); await frame();
    r.dismissed = text('toast-value');
    return r;
  });

  await step('dateInput', async () => {
    const box = within('when-value');
    const [year, month, day] = box.querySelectorAll('input');
    year.focus(); await frame();
    for (const digit of '2026') { keyHere(digit); await frame(); }
    const moved = `focus=${document.activeElement.getAttribute('aria-label')} year=${year.value}`;
    for (const digit of '230') { keyHere(digit); await frame(); }
    const typed = `${text('when-value')} fields=${year.value}-${month.value}-${day.value}`;
    keyHere('ArrowUp'); await frame();
    return { moved, typed, stepped: text('when-value') };
  });

  await step('datePicker', async () => {
    const box = within('due-value');
    const trigger = box.querySelector('[data-armature-part="Trigger"]');
    trigger.click(); await frame();
    const opened = `days=${[...box.querySelectorAll('[role=gridcell]')].filter(shown).length > 27}`;
    box.querySelector('[role=gridcell][data-value="2026-11-20"]').click(); await frame();
    const fields = [...box.querySelectorAll('input')].map(i => i.value).join('-');
    return { opened, picked: `${text('due-value')} fields=${fields} open=${trigger.getAttribute('aria-expanded')} focus=${who()}` };
  });

  await step('mentions', async () => {
    const box = within('note-value');
    const input = box.querySelector('input');
    const options = () => [...box.querySelectorAll('[role=option]')].filter(shown).map(o => o.dataset.value).join(',');
    await type(input, 'ping @al');
    const suggested = `${options()} open=${input.getAttribute('aria-expanded')}`;
    key(input, 'Enter'); await frame();
    return { suggested, inserted: `${text('note-value')}| field="${input.value}" open=${input.getAttribute('aria-expanded')}` };
  });

  await step('carousel', async () => {
    const box = within('slide-value');
    const next = box.querySelector('[aria-label="Next slide"]');
    const viewport = box.querySelector('[data-armature-part="Viewport"]');
    next.click(); await frame();
    const r = { next: `${text('slide-value')} prev=${box.querySelector('[aria-label="Previous slide"]').getAttribute('aria-disabled')}` };
    const at = viewport.getBoundingClientRect();
    ptr(viewport, 'pointerdown', at.left + 150, at.top + 10);
    ptr(window, 'pointermove', at.left + 40, at.top + 10); await frame();
    ptr(window, 'pointerup', at.left + 40, at.top + 10); await frame();
    r.dragged = `${text('slide-value')} next=${next.getAttribute('aria-disabled')}`;
    return r;
  });

  await step('scrollArea', async () => {
    const box = section('Scroll area');
    const viewport = box.querySelector('[data-armature-part="Viewport"]');
    const place = box.querySelector('.thumb-place');
    await frame();
    const start = `top=${place.style.top} tall=${parseFloat(place.style.height) < 60}`;
    viewport.scrollTop = viewport.scrollHeight; await wait(150);
    const end = Math.round(parseFloat(place.style.top) + parseFloat(place.style.height));
    const thumb = box.querySelector('[data-armature-part="Thumb"]').getBoundingClientRect();
    ptr(box.querySelector('[data-armature-part="Thumb"]'), 'pointerdown', thumb.left + 2, thumb.top + 2);
    ptr(window, 'pointermove', thumb.left + 2, thumb.top - 200); await wait(150);
    ptr(window, 'pointerup', thumb.left + 2, thumb.top - 200); await frame();
    return { start, thumbReachesTheEnd: end === 120, dragScrollsBack: viewport.scrollTop === 0 };
  });

  await step('cropper', async () => {
    const box = within('crop-value');
    const selection = box.querySelector('[data-armature-part="Selection"]');
    const drag = async (target, dx, dy) => {
      const at = target.getBoundingClientRect(); const x = at.left + at.width / 2, y = at.top + at.height / 2;
      ptr(target, 'pointerdown', x, y); ptr(window, 'pointermove', x + dx, y + dy); await frame(); ptr(window, 'pointerup', x + dx, y + dy); await frame();
    };
    const shownWidth = box.querySelector('[data-armature-part="Area"]').getBoundingClientRect().width;
    await drag(box.querySelector('[data-armature-part="Handle"][data-value="e"]'), -shownWidth / 2, 0);
    const resized = text('crop-value');
    await drag(selection, shownWidth / 4, 0);
    key(selection, 'ArrowDown'); await frame();
    return { resized, moved: text('crop-value') };
  });

  await step('files', async () => {
    const box = within('files-value');
    const zone = box.querySelector('[data-armature-part="Dropzone"]');
    const data = new DataTransfer();
    data.items.add(new File(['x'], 'photo.png', { type: 'image/png' }));
    data.items.add(new File(['x'], 'notes.txt', { type: 'text/plain' }));
    zone.dispatchEvent(new DragEvent('dragenter', { bubbles: true, cancelable: true, dataTransfer: data })); await frame();
    const over = zone.hasAttribute('data-active');
    zone.dispatchEvent(new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: data })); await frame();
    const dropped = `${text('files-value')} | said="${document.getElementById('armature-live-polite')?.textContent}"`;
    box.querySelector('[aria-label="Remove"]').click(); await frame();
    return { over, dropped, removed: text('files-value') };
  });

  await step('waterfall', async () => {
    const box = within('span-value');
    const rows = () => [...box.querySelectorAll('[role=treeitem]')].map(r => `${r.dataset.value}@${r.getAttribute('aria-level')}`).join(' ');
    const bar = box.querySelectorAll('.trace-bar')[1];
    const r = { start: `${rows()} | api bar ${bar.style.left} ${bar.style.width}` };
    const first = box.querySelector('[role=treeitem]');
    key(first, 'ArrowDown'); await frame(); keyHere('ArrowLeft'); await frame();
    r.collapsed = rows();
    keyHere('Enter'); await frame();
    r.selected = text('span-value');
    return r;
  });

  return out;
}
