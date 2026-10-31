import React, {useState} from 'react';
import './search-fragments.css';

export type SearchChunk = {text: string; start: number | null; end: number | null; score?: number; stale?: boolean; field?: 'description' | 'body'};
export type SearchResult = {id: string; title: string; chunks?: SearchChunk[]; missing?: boolean};

function Hit({result, chunk, onOpen}: {result: SearchResult; chunk: SearchChunk | null; onOpen: (id:string, chunk:SearchChunk|null)=>void}) {
  const [expanded, setExpanded] = useState(false);
  const chars = Array.from(chunk?.text ?? '');
  const long = chars.length > 240;
  const text = long && !expanded ? chars.slice(0, 240).join('').trimEnd() + '…' : chunk?.text;
  return <div className="search-hit-wrap"><button className="search-hit" disabled={result.missing} onClick={()=>onOpen(result.id, chunk?.stale ? null : chunk)} title={chunk?.stale ? 'Текст изменился; открыть источник' : 'Открыть источник'}>
    <strong>{result.title}{result.missing && <small> · Источник удалён</small>}</strong>
    {chunk && <span className="search-hit-text">{text}{chunk.stale && <small> · Текст изменился</small>}</span>}
  </button>{long && <button type="button" className="search-hit-expand" aria-expanded={expanded} onClick={()=>setExpanded(!expanded)}>{expanded ? 'Свернуть' : 'Показать полностью'}</button>}</div>;
}

export function SearchResults({results, onOpen}: {results: SearchResult[] | null; onOpen: (id: string, chunk: SearchChunk | null) => void}) {
  if (results === null) return null;
  if (results.length === 0) return <p>Ничего не найдено</p>;
  return <div className="results">{results.flatMap((result, resultIndex) => {
    const chunks = result.chunks?.length ? result.chunks : [null];
    return chunks.map((chunk, chunkIndex) => <Hit result={result} chunk={chunk} onOpen={onOpen} key={`${result.id}-${resultIndex}-${chunkIndex}-${chunk?.start ?? ''}-${chunk?.text ?? ''}`}/>);
  })}</div>;
}
