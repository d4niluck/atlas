import React, {useEffect, useRef} from 'react';

/** Offsets refer to Unicode code points in the original text, not UTF-16 units. */
export function SearchExcerpt({text, start, end}: {text: string; start: number; end: number}) {
  const mark = useRef<HTMLElement>(null);
  useEffect(() => { mark.current?.scrollIntoView({block: 'center'}); }, [text, start, end]);
  const chars = Array.from(text);
  const a = Math.max(0, Math.min(chars.length, start));
  const b = Math.max(a, Math.min(chars.length, end));
  return <pre style={{whiteSpace:'pre-wrap',overflowWrap:'anywhere'}}>{chars.slice(0,a).join('')}<mark ref={mark}>{chars.slice(a,b).join('')}</mark>{chars.slice(b).join('')}</pre>;
}
