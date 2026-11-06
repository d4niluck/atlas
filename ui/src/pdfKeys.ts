export function pdfPageDirection(event: Pick<KeyboardEvent, 'key'|'altKey'|'ctrlKey'|'metaKey'|'shiftKey'|'defaultPrevented'|'target'>, active = true): number {
  if (!active || event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return 0;
  const target = event.target as HTMLElement | null;
  if (target?.closest?.('input, textarea, select, [contenteditable]:not([contenteditable="false"])')) return 0;
  return event.key === 'ArrowLeft' ? -1 : event.key === 'ArrowRight' ? 1 : 0;
}
