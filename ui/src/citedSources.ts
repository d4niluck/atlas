function dollarOpensMath(text:string,start:number,mark:string):boolean {
 for (let i = start + mark.length; i < text.length; i++) {
  if (mark === '$' && text[i] === '\n') return false;
  if (text[i] === '\\') { i++; continue; }
  if (text.startsWith(mark, i)) {
   const numeric = /^[+-]?\d/.test(text.slice(start + mark.length));
   return !(mark === '$' && numeric && /^\d/.test(text.slice(i + 1)));
  }
 }
 return false;
}
/** Preserve the original source numbers used by the model, including gaps. */
export function citedSources<T>(answer: string | null | undefined, sources: T[]): {number: number; source: T}[] {
  const cited = new Set<number>();
  const text = answer ?? '';
  const state: {code:{mark:string; length:number; fenced:boolean} | null} = {code:null};
  let math = '';
  for (let i = 0; i < text.length;) {
    const char = text[i];
    if (char === '\\' && i + 1 < text.length) {
      const next = text[i + 1];
      if (!state.code) { if (!math && (next === '(' || next === '[')) math = next; else if ((math === '(' && next === ')') || (math === '[' && next === ']')) math = ''; }
      i += 2; continue;
    }
    if (char === '`' || (char === '~' && (state.code?.mark === '~' || text.slice(i, i + 3) === '~~~'))) {
      let end = i + 1; while (text[end] === char) end++;
      const length = end - i, indent = text.slice(text.lastIndexOf('\n', i - 1) + 1, i);
      const atStart = /^ {0,3}$/.test(indent), closingTail = /^[ \t\r]*$/.test(text.slice(end, text.indexOf('\n', end) < 0 ? text.length : text.indexOf('\n', end)));
      if (!math) { if (state.code?.mark === char && (state.code.fenced ? atStart && length >= state.code.length && closingTail : length === state.code.length)) state.code = null; else if (!state.code) {const fenced = length >= 3 && atStart; if (char === '`' || fenced) state.code = {mark: char, length, fenced};} }
      i = end; continue;
    }
    if (char === '$' && !state.code) { const mark = text[i + 1] === '$' ? '$$' : '$'; if (!math && dollarOpensMath(text, i, mark)) math = mark; else if (math === mark) math = ''; i += mark.length; continue; }
    if (char === '[' && !state.code && !math && text[i - 1] !== '!') {
      const match = /^\[(\d+)\]/.exec(text.slice(i));
      const next = match ? text.slice(i + match[0].length) : '';
      const reference = /^\[[^\]\n]*\]/.exec(next);
      const referenceId = reference?.[0].slice(1, -1);
      const markdownReference = referenceId !== undefined && !/^\s*\d+(?:\s*[,;，–—-]\s*\d+)*\s*$/.test(referenceId);
      if (match && next[0] !== '(' && !markdownReference) {
        const number = Number(match[1]);
        if (Number.isSafeInteger(number) && number >= 1 && number <= sources.length) cited.add(number);
        i += match[0].length; continue;
      }
    }
    i++;
  }
  return sources.flatMap((source, index) => cited.has(index + 1) ? [{number: index + 1, source}] : []);
}
