import React from 'react';

type Props = React.AnchorHTMLAttributes<HTMLAnchorElement> & {node?: unknown};
type TauriWindow = Window & {__TAURI__?: {core: {invoke: (command: string, args: {url: string}) => Promise<unknown>}}};

export function MarkdownLink({href, children}:Props){
 let safe: string | undefined;
 try { const url = new URL(href || ''); if(url.protocol==='http:' || url.protocol==='https:') safe=url.href; } catch { /* Relative and malformed links are not external URLs. */ }
 if(!safe) return <span>{children}</span>;
 const url=safe;
 return <a href={url} target="_blank" rel="noopener noreferrer" onClick={event=>{
  const tauri=(window as TauriWindow).__TAURI__;
  if(tauri){event.preventDefault();void tauri.core.invoke('open_external_url',{url}).catch(()=>window.alert('Не удалось открыть ссылку'));}
 }}>{children}</a>;
}
