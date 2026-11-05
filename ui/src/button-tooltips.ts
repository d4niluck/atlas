import './button-tooltips.css';

let active: HTMLButtonElement | null = null;
let originalTitle: string | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
let tooltip: HTMLDivElement | null = null;
function clear() {
 if (timer) clearTimeout(timer);
 timer = undefined;
 tooltip?.remove(); tooltip = null;
 if (active && originalTitle !== null && !active.hasAttribute('title')) active.setAttribute('title', originalTitle);
 active = null; originalTitle = null;
}
document.addEventListener('pointerover', event => {
 const button = (event.target as Element | null)?.closest('button');
 if (!button || button === active) return;
 clear();
 const label = button.getAttribute('title') || button.getAttribute('aria-label') || button.textContent?.trim();
 if (!label || !/\p{L}/u.test(label)) return;
 active = button;
 originalTitle = button.getAttribute('title');
 if (originalTitle !== null) button.removeAttribute('title');
 timer = setTimeout(() => {
  if (active !== button || !button.isConnected) return;
  tooltip = document.createElement('div');
  tooltip.className = 'button-tooltip'; tooltip.setAttribute('role', 'tooltip'); tooltip.textContent = label;
  document.body.appendChild(tooltip);
  const rect = button.getBoundingClientRect(), tip = tooltip.getBoundingClientRect();
  tooltip.style.left = `${Math.max(8, Math.min(innerWidth - tip.width - 8, rect.left + (rect.width - tip.width) / 2))}px`;
  tooltip.style.top = `${rect.bottom + tip.height + 8 <= innerHeight ? rect.bottom + 6 : Math.max(8, rect.top - tip.height - 6)}px`;
 }, 2000);
});
document.addEventListener('pointerout', event => { if (active && !active.contains(event.relatedTarget as Node | null)) clear(); });
document.addEventListener('pointerdown', clear);
document.addEventListener('scroll', clear, true);
window.addEventListener('blur', clear);
