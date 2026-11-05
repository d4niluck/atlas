export type Format = 'bold'|'italic'|'strike'|'code'|'link'|'heading'|'list'|'ordered'|'quote';
export function applyFormat(text:string,start:number,end:number,kind:Format) {
 const selected=text.slice(start,end);
 if(kind==='heading'||kind==='list'||kind==='ordered'||kind==='quote') {
  const first=start===0?0:text.lastIndexOf('\n',start-1)+1;
  const last=text.indexOf('\n',end); const boundary=last<0?text.length:last;
  const lines=text.slice(first,boundary).split('\n');
  const formatted=lines.map((line,i)=>`${kind==='heading'?'## ':kind==='list'?'- ':kind==='ordered'?`${i+1}. `:'> '}${line}`).join('\n');
  return {text:text.slice(0,first)+formatted+text.slice(boundary),start:first,end:first+formatted.length};
 }
 if(kind==='link') {
  const label=selected||'текст ссылки'; const content=`[${label}](https://)`;
  return {text:text.slice(0,start)+content+text.slice(end),start:start+label.length+3,end:start+label.length+11};
 }
 const mark=kind==='bold'?'**':kind==='italic'?'*':kind==='strike'?'~~':'`';
 const content=selected||'текст';
 return {text:text.slice(0,start)+mark+content+mark+text.slice(end),start:start+mark.length,end:start+mark.length+content.length};
}
