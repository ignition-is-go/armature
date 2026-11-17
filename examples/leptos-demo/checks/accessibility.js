async () => {
  const frame = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(r, 50))));
  const within = (id) => document.getElementById(id).parentElement;
  // One of two boxes checked, so the box for the group is mixed.
  within('alerts-value').querySelectorAll('[role=checkbox]')[2].click(); await frame();
  // A field left with a bad value.
  const email = within('email-value').querySelector('input');
  email.focus(); email.value = 'ada'; email.dispatchEvent(new Event('input', { bubbles: true })); await frame();
  within('email-value').querySelectorAll('input')[1].focus(); await frame();
  // A toggle down, a row dropped so something is announced, and a toast showing.
  within('align-value').querySelector('[role=button]').click(); await frame();
  const row = document.querySelector('[role=listitem][data-value="milk"], [data-armature-part="Item"][data-value="milk"]');
  if (row) { row.focus(); for (const key of [' ', 'ArrowDown', ' ']) { row.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true })); await frame(); } }
  within('toast-value').querySelector('[role=button]').click(); await frame();
  await new Promise(r => setTimeout(r, 150));
  // A popover open, so its title and description name and describe it.
  document.querySelector('[data-armature-part="Trigger"][aria-label="Share"]').click(); await frame();
}
