import React, {isValidElement} from 'react';
import {MarkdownPre} from './MermaidPre';
import {PythonCode} from './PythonCode';

export function MarkdownCodePre({children, ...props}:React.HTMLAttributes<HTMLPreElement> & {node?:unknown}) {
 if(isValidElement<{className?:string;children?:React.ReactNode; 'data-atlas-search-range'?:string}>(children)
   && /(?:^|\s)language-(?:python|py)(?:\s|$)/i.test(children.props.className || '')
   && typeof children.props.children==='string' && !children.props['data-atlas-search-range']) {
  const {node: _node, ...preProps}=props;
  return <pre {...preProps}><PythonCode source={children.props.children}/></pre>;
 }
 // Mermaid and search highlights retain their existing rendering and navigation.
 return <MarkdownPre {...props}>{children}</MarkdownPre>;
}
