/** Treat untrusted files as text only when they are valid UTF-8 without binary control bytes. */
export function decodePlainText(bytes: Uint8Array): string | null {
  if (bytes.some(byte => byte === 0 || (byte < 32 && byte !== 9 && byte !== 10 && byte !== 13))) return null;
  try { return new TextDecoder('utf-8', {fatal: true}).decode(bytes); } catch { return null; }
}
