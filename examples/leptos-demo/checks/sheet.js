async () => {
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const wait = (ms) => new Promise(r => setTimeout(r, ms));
  const text = (id) => document.getElementById(id).textContent;
  const key = (el, key) => { el.focus(); el.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })); };
  const ptr = (target, type, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, cancelable: true, clientX: 50, clientY: y, button: 0, pointerType: 'mouse' }));
  const trigger = document.querySelector('[data-armature-part="Trigger"][aria-label="Details"]');
  const panel = document.getElementById(trigger.getAttribute('aria-controls'));
  const grip = panel.querySelector('[data-armature-part="Handle"]');
  const height = () => Math.round(document.querySelector('.sheet-slot').getBoundingClientRect().height);
  const out = { visible: text('visible-value') === `Visible: ${Math.round(visualViewport.width)}x${Math.round(visualViewport.height)}` };
  key(trigger, 'Enter'); await frame();
  out.opened = `${text('sheet-value')} height=${height()} modal=${panel.getAttribute('aria-modal')} gripFocused=${document.activeElement === grip} gripName=${grip.getAttribute('aria-label')}`;
  const top = grip.getBoundingClientRect().top;
  ptr(grip, 'pointerdown', top);
  ptr(window, 'pointermove', top - 80); await wait(400);
  out.following = height();
  ptr(window, 'pointermove', top - 150); await wait(400);
  ptr(window, 'pointerup', top - 150); await frame();
  out.snappedTall = `${text('sheet-value')} height=${height()}`;
  key(grip, 'ArrowDown'); await frame();
  out.arrowDown = `${text('sheet-value')} height=${height()}`;
  key(grip, 'ArrowDown'); await frame();
  out.dismissed = `${text('sheet-value')} shown=${getComputedStyle(panel).display !== 'none'} focusOnTrigger=${document.activeElement === trigger}`;
  return out;
}