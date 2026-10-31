import React from 'react';

type Props = {name:string; active:boolean; busy:boolean; onSelect:()=>void; onClose:()=>void};
export function WorkspaceTab({name, active, busy, onSelect, onClose}:Props){
 return <span className={'workspace-tab '+(active?'active':'')}>
  <button type="button" className={active?'active':''} onClick={onSelect}>{name}</button>
  <button type="button" className="workspace-tab-close" aria-label={`Закрыть вкладку ${name}`} title="Закрыть вкладку" disabled={busy} onClick={onClose}>×</button>
 </span>;
}
