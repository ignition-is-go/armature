async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 40))));
  const key = (key, init = {}) => { const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); document.activeElement.dispatchEvent(e); return e.defaultPrevented; };
  const trigger = document.querySelector('[data-armature-part="Trigger"][aria-label="Confirm"]');
  const dialog = document.getElementById(trigger.getAttribute('aria-controls'));
  const backdrop = dialog.previousElementSibling;
  const shown = () => getComputedStyle(dialog).display !== 'none';
  const who = () => document.activeElement.getAttribute('data-armature-part') || document.activeElement.id || document.activeElement.tagName;
  out.start = `shown=${shown()} backdrop=${getComputedStyle(backdrop).display}`;
  trigger.focus(); trigger.click(); await frame();
  out.opened = `shown=${shown()} modal=${dialog.getAttribute('aria-modal')} focus=${who()} expanded=${trigger.getAttribute('aria-expanded')}`;
  const order = [];
  for (let i = 0; i < 4; i++) { key('Tab'); await frame(); order.push(who()); }
  out.tabCycle = order.join(' > ');
  key('Tab', { shiftKey: true }); await frame();
  out.shiftTab = who();
  key('Escape'); await frame();
  out.escape = `shown=${shown()} focus=${who()}`;
  trigger.click(); await frame();
  backdrop.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0 })); await frame();
  out.backdrop = `shown=${shown()} focus=${who()}`;
  trigger.click(); await frame();
  dialog.querySelector('[data-armature-part="Close"]').click(); await frame();
  out.close = `shown=${shown()} focus=${who()}`;
  const a = window.armature; const ref = a.snapshot().find(s => s.label === 'Confirm').ref;
  a.act(ref, 'open'); await frame();
  out.agentOpen = `shown=${shown()} expanded=${a.snapshot().find(s => s.label === 'Confirm').expanded}`;
  a.act(ref, 'close'); await frame();
  // A dialog that is not dismissible stays through Escape and an outside press.
  const terms = document.querySelector('[data-armature-part="Trigger"][aria-label="Terms"]');
  const sheet = document.getElementById(terms.getAttribute('aria-controls'));
  const up = () => getComputedStyle(sheet).display !== 'none';
  terms.focus(); terms.click(); await frame();
  key('Escape'); await frame();
  const afterEscape = up();
  sheet.previousElementSibling.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, button: 0 })); await frame();
  out.fixed = `escape=${afterEscape} outside=${up()}`;
  sheet.querySelector('[data-armature-part="Close"]').click(); await frame();
  out.fixedClose = `shown=${up()}`;
  // Tooltips sharing warmth: the second opens without its delay.
  const [bold, italic] = document.querySelectorAll('#warm-tips [data-armature-part="Trigger"]');
  const tipOf = (el) => document.getElementById(el.getAttribute('aria-describedby'));
  const seen = (el) => getComputedStyle(tipOf(el)).display !== 'none';
  const move = (el, type) => el.dispatchEvent(new PointerEvent(type, { bubbles: false, cancelable: true, clientX: 0, clientY: 0, button: 0, pointerType: 'mouse' }));
  bold.scrollIntoView(); await frame();
  move(bold, 'pointerenter'); await new Promise(r => setTimeout(r, 150));
  const early = seen(bold);
  await new Promise(r => setTimeout(r, 500));
  const first = seen(bold);
  move(bold, 'pointerleave');
  // The pointer moves well away, so it is not on its way to the tooltip.
  window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, clientX: 5000, clientY: 5000, pointerType: 'mouse' })); await frame();
  const gone = !seen(bold);
  move(italic, 'pointerenter'); await frame();
  out.warm = `early=${early} first=${first} secondAtOnce=${seen(italic)} firstGone=${gone}`;
  move(italic, 'pointerleave'); await frame();
  // A disabled menu trigger never opens by hand.
  const locked = document.querySelector('[data-armature-part="Trigger"][aria-label="Locked actions"]');
  locked.scrollIntoView(); locked.focus(); locked.click(); await frame();
  key('ArrowDown'); await frame();
  out.lockedMenu = `expanded=${locked.getAttribute('aria-expanded')} disabled=${locked.getAttribute('aria-disabled') ?? locked.disabled}`;
  // A modal popover is marked modal.
  const details = document.querySelector('[data-armature-part="Trigger"][aria-label="Modal details"]');
  const panel = document.getElementById(details.getAttribute('aria-controls'));
  details.focus(); details.click(); await frame();
  out.modalPopover = `shown=${getComputedStyle(panel).display !== 'none'} modal=${panel.getAttribute('aria-modal')}`;
  out.modalPopoverParts = [...panel.querySelectorAll('[data-armature-part]')].map(el => el.getAttribute('data-armature-part')).join(',');
  panel.querySelector('[data-armature-part="Close"]').click(); await frame();
  out.modalPopoverClosed = `shown=${getComputedStyle(panel).display !== 'none'}`;
  // Apple key conventions, with the page presenting as a Mac.
  const real = Object.getOwnPropertyDescriptor(Navigator.prototype, 'platform');
  const options = [...document.querySelectorAll('[aria-label="Fruit"] [role=option], [role=listbox][aria-label="Fruit"] [role=option]')];
  const at = () => document.activeElement.textContent.trim();
  options[0].scrollIntoView(); options[0].focus(); await frame();
  key('ArrowDown', { metaKey: true }); await frame();
  const linux = at();
  Object.defineProperty(Navigator.prototype, 'platform', { get: () => 'MacIntel', configurable: true });
  options[0].focus(); await frame();
  key('ArrowDown', { metaKey: true }); await frame();
  const last = at();
  key('p', { ctrlKey: true }); await frame();
  const stepped = at();
  key('ArrowUp', { metaKey: true }); await frame();
  out.appleKeys = `elsewhere=${linux} cmdDown=${last} ctrlP=${stepped} cmdUp=${at()}`;
  Object.defineProperty(Navigator.prototype, 'platform', real);
  return out;
}