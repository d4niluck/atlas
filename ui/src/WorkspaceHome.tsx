import React, {useState} from 'react';

type Workspace = {id: string; name: string};
type Props = {workspaces: Workspace[]; onOpen: (id:string) => void; onCreate: () => void; onDelete: (id:string) => Promise<void>};
export function WorkspaceHome({workspaces, onOpen, onCreate, onDelete}: Props) {
 const [confirm, setConfirm] = useState<Workspace|null>(null);
 const [busy, setBusy] = useState(false);
 const [error, setError] = useState('');
 async function remove() {
  if (!confirm || busy) return;
  setBusy(true);
  try { await onDelete(confirm.id); setConfirm(null); setError(''); }
  catch (e) { setError(e instanceof Error ? e.message : 'Не удалось удалить пространство'); }
  finally { setBusy(false); }
 }
 return <main className="workspace-home"><h1>Ваши пространства</h1><button className="primary" onClick={onCreate}>＋ Создать пространство</button>
  {workspaces.map(item => <div className="workspace-home-row" key={item.id}>
   <button onClick={() => onOpen(item.id)}>{item.name}</button><button aria-label={`Удалить пространство ${item.name}`} onClick={() => { setConfirm(item); setError(''); }}>Удалить</button>
  </div>)}
  {confirm && <div className="overlay"><section className="editor workspace-delete" role="dialog" aria-modal="true" aria-label="Подтверждение удаления">
   <h2>Удалить «{confirm.name}»?</h2><p>Все заметки и файлы этого пространства будут удалены.</p>
   {error && <p role="alert">{error}</p>}
   <div><button onClick={() => setConfirm(null)} disabled={busy}>Отмена</button><button onClick={remove} disabled={busy}>Удалить пространство</button></div>
  </section></div>}
 </main>;
}
