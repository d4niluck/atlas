import React from 'react';
import type {SearchChunk, SearchResult} from './SearchResults';
import './menu-search-results.css';

export function MenuSearchResults({results, onOpen}: {results: SearchResult[] | null; onOpen: (id: string, chunk: SearchChunk | null) => void}) {
  if (results === null) return null;
  if (!results.length) return <p className="menu-results-empty">Ничего не найдено</p>;
  return <div className="menu-results">{results.flatMap((result, resultIndex) => {
    const chunks = result.chunks?.length ? result.chunks : [null];
    return chunks.map((chunk, index) => {
      const chars = Array.from(chunk?.text ?? '');
      const short = chars.length > 180 ? chars.slice(0, 180).join('').trimEnd() + '…' : chunk?.text;
      return <article className="menu-result" key={`${result.id}-${resultIndex}-${index}`}>
        <div className="menu-result-heading"><strong>{result.title}</strong><button type="button" disabled={result.missing} onClick={() => onOpen(result.id, chunk?.stale ? null : chunk)} aria-label={`Открыть источник: ${result.title}`} title="Открыть источник">↗<span className="menu-result-sr-only">Открыть источник</span></button></div>
        {result.missing && <small>Источник удалён</small>}
        {chunk && <details><summary><span className="menu-result-short">{short}</span><span className="menu-result-toggle">Развернуть</span></summary><p>{chunk.text}</p></details>}
        {chunk?.stale && <small>Текст изменился</small>}
      </article>;
    });
  })}</div>;
}
