import React, {useRef, useState} from 'react';

type Props = {name: string; onRename: (name: string) => Promise<void>};
export function WorkspaceTitle({name, onRename}: Props) {
 const [editing, setEditing] = useState(false);
 const [value, setValue] = useState(name);
 const [error, setError] = useState('');
 const saving = useRef(false);
 const cancelled = useRef(false);
 async function save(next: string) {
  if (saving.current || cancelled.current) return;
  if (!next.trim() || next.trim() === name) { setEditing(false); return; }
  saving.current = true;
  try { await onRename(next.trim()); setEditing(false); setError(''); }
  catch { setError('Не удалось переименовать пространство'); }
  finally { saving.current = false; }
 }
 if (editing) return <span className="workspace-title-edit">
  <input autoFocus aria-label="Новое название пространства" value={value} onChange={e => setValue(e.target.value)} maxLength={100}
   onBlur={e => void save(e.currentTarget.value)} onKeyDown={e => {
    if (e.key === 'Enter') { e.preventDefault(); e.currentTarget.blur(); }
    if (e.key === 'Escape') { e.preventDefault(); cancelled.current = true; setEditing(false); setError(''); }
   }}/>
  {error && <span role="alert">{error}</span>}
 </span>;
 return <button className="workspace-title" title="Переименовать пространство" aria-label={`Переименовать пространство ${name}`} onClick={() => { cancelled.current = false; setValue(name); setEditing(true); }}>{name}</button>;
}
