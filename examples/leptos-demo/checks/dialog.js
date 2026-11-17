async () => {
  const out = {};
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 40))));
  const key = (key, init = {}) => { const e = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init }); document.activeElement.dispatchEvent(e); return e.defaultPrevented; };
  const trigger = document.querySelector('[data-armature-part="Trigger"][aria-label="Confirm"]');
  const dialog = document.getElementById(trigger.getAttribute('aria-controls'));
  const backdrop = document.querySelector('[data-armature-part="Backdrop"]');
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
  return out;
}