export async function readPreviewWindow(response: Response, around = 0): Promise<{text:string; offset:number; truncated:boolean}> {
  const limit = 128 * 1024;
  const offset = Math.max(0, around - Math.floor(limit / 2));
  const reader = response.body?.getReader();
  if (!reader) throw new Error('Не удалось прочитать файл');
  const decoder = new TextDecoder('utf-8', {fatal:true});
  let position = 0, text = '', truncated = false;
  while (true) {
    const part = await reader.read();
    const decoded = part.done ? decoder.decode() : decoder.decode(part.value, {stream:true});
    const start = Math.max(0, offset - position), end = Math.min(decoded.length, offset + limit - position);
    if (end > start) text += decoded.slice(start, end);
    position += decoded.length;
    if (position > offset + limit) { truncated = true; await reader.cancel(); break; }
    if (part.done) break;
  }
  return {text, offset, truncated: truncated || offset > 0};
}
