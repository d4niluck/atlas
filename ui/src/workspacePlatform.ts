export function isWindows(userAgent: string): boolean {
 return /Windows NT/i.test(userAgent);
}
