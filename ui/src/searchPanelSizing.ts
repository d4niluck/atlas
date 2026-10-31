export const SEARCH_PANEL_DEFAULT = 360;
export function clampSearchPanelWidth(width:number, viewport:number):number {
  const max = Math.max(0, viewport - 40);
  const min = Math.min(280, max);
  return Math.min(max, Math.max(min, Number.isFinite(width) ? width : SEARCH_PANEL_DEFAULT));
}
export function searchPanelWidthFromPointer(right:number, pointerX:number, viewport:number):number {
  return clampSearchPanelWidth(right-pointerX,viewport);
}
