import React, {useEffect,useState} from 'react';
import DOMPurify from 'dompurify';
import {highlightPython} from './python-syntax';
import './python-syntax.css';

export function PythonCode({source}: {source:string}) {
 const [highlighted,setHighlighted]=useState<{source:string;html:string}|null>(null);
 useEffect(()=>{
  let active=true;
  void highlightPython(source).then(html=>{
   if(active) setHighlighted(html===null?null:{source,html:DOMPurify.sanitize(html,{ALLOWED_TAGS:['span'],ALLOWED_ATTR:['class']})});
  }).catch(()=>{if(active)setHighlighted(null)});
  return()=>{active=false};
 },[source]);
 if(highlighted?.source===source) return <code className="python-code" dangerouslySetInnerHTML={{__html:highlighted.html}}/>;
 return <code className="python-code">{source}</code>;
}
