import React, {useEffect, useState} from 'react';
import type {SearchResult} from './SearchResults';
import './search-history.css';

type Entry = {agentic?: boolean | null; error?: string | null; query: string; mode: 'lexical' | 'semantic' | 'hybrid' | 'rag'; results: SearchResult[]; answer?: string; sources?: {id:string;title:string;text:string;start:number|null;end:number|null;field?:'body'|'description';stale?:boolean;missing?:boolean}[]; created_at: string; object_ids: string[] | null; scope_known: boolean; order_exact: boolean};
export function countHistoryResults(results: SearchResult[]) {
  return results.reduce((count, result) => count + (result.chunks?.length || 1), 0);
}
export function historyPreview(mode: Entry['mode'], answer?: string) {
  return mode === 'rag' ? `Ответ: ${answer?.trim() || 'нет ответа'}` : mode === 'semantic' ? 'По смыслу' : mode === 'hybrid' ? 'По словам и смыслу' : 'По словам';
}
export function SearchHistory({prefix, revision, onSelect}: {prefix: string; revision: number; onSelect: (entry: Entry) => void}) {
  const [items, setItems] = useState<Entry[]>([]);
  const [error, setError] = useState('');
  useEffect(() => {
    let active = true;
    setError('');
    fetch(prefix + '/history').then(response => {
      if (!response.ok) throw new Error('Не удалось загрузить историю');
      return response.json() as Promise<{items: Entry[]}>;
    }).then(data => {if (active) setItems(data.items)}).catch(() => {if (active) setError('Не удалось загрузить историю')});
    return () => {active = false};
  }, [prefix, revision]);
  if (error) return <p role="alert">{error}</p>;
  if (!items.length) return <p>История пуста</p>;
  return <div className="results">{items.map((item, index) => <button className="result history-entry" key={`${item.created_at}-${index}`} onClick={() => onSelect(item)} title={`Открыть ответ и результаты: ${countHistoryResults(item.results)}`}><strong>{item.query}</strong><span className="history-detail"><span className="history-preview">{historyPreview(item.mode, item.answer)}</span><span className="history-count">Результатов: {countHistoryResults(item.results)}</span></span></button>)}</div>;
}
