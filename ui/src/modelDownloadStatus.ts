export function mergeDownloadStatus(current: string, next: string): string {
 return next === 'idle' && current.startsWith('error:') ? current : next;
}
