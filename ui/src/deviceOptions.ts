export type Device = 'cpu'|'metal';
export function availableDevices(supported:readonly string[]|undefined):Device[] {
 return supported?.includes('metal')?['cpu','metal']:['cpu'];
}
