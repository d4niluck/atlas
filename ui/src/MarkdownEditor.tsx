import React, {useRef} from 'react';
import {applyFormat, type Format} from './markdown-format';
import './markdown-editor.css';

type Props={text:string;onChange:(text:string)=>void};
const actions:{kind:Format;label:string;symbol:string}[]=[
 {kind:'bold',label:'Жирный',symbol:'Ж'}, {kind:'italic',label:'Курсив',symbol:'К'},
 {kind:'strike',label:'Зачёркнутый',symbol:'S'}, {kind:'heading',label:'Заголовок',symbol:'H'},
 {kind:'list',label:'Маркированный список',symbol:'•'}, {kind:'ordered',label:'Нумерованный список',symbol:'1.'},
 {kind:'quote',label:'Цитата',symbol:'❞'}, {kind:'code',label:'Код',symbol:'</>'},
 {kind:'link',label:'Ссылка',symbol:'↗'},
];
export function MarkdownEditor({text,onChange}:Props){
 const input=useRef<HTMLTextAreaElement>(null);
 function format(kind:Format){
  const element=input.current;if(!element)return;
  const result=applyFormat(text,element.selectionStart,element.selectionEnd,kind);
  // Use the browser's editing command so formatting participates in native undo.
  let first=0;while(first<text.length&&first<result.text.length&&text[first]===result.text[first])first++;
  let end=text.length,nextEnd=result.text.length;
  while(end>first&&nextEnd>first&&text[end-1]===result.text[nextEnd-1]){end--;nextEnd--;}
  element.focus();element.setSelectionRange(first,end);
  if(!document.execCommand('insertText',false,result.text.slice(first,nextEnd)))onChange(result.text);
  requestAnimationFrame(()=>{element.focus();element.setSelectionRange(result.start,result.end)});
 }
 return <div className="markdown-editor"><div className="markdown-toolbar" role="toolbar" aria-label="Форматирование текста">
  {actions.map(a=><button key={a.kind} type="button" title={a.label} aria-label={a.label} onMouseDown={e=>e.preventDefault()} onClick={()=>format(a.kind)}>{a.symbol}</button>)}
 </div><textarea ref={input} value={text} onChange={e=>onChange(e.target.value)} placeholder="Начните писать…" aria-label="Текст заметки"/></div>;
}
