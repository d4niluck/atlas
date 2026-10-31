/** Apply only numeric, prevalidated font sizes to sanitized SVG text nodes. */
export function applyMermaidFontSizes(svg:string, sizes:number[]):string {
 if (!sizes.length) return svg;
 const doc=new DOMParser().parseFromString(svg,'image/svg+xml');
 if (doc.querySelector('parsererror') || doc.documentElement.localName !== 'svg') throw Error('Invalid SVG');
 const found=new Set<number>();
 for (const text of Array.from(doc.querySelectorAll('text'))) {
  const walker=doc.createTreeWalker(text,NodeFilter.SHOW_TEXT);
  const nodes:Text[]=[];while(walker.nextNode())nodes.push(walker.currentNode as Text);
  const combined=nodes.map(node=>node.data).join('');
  const matches=[...combined.matchAll(/\uE000(\d+)\uE001([\s\S]*?)\uE002/g)];
  if (!matches.length) continue;
  const ranges=matches.map(match=>{
   const id=Number(match[1]); if(!Number.isInteger(id)||id<0||id>=sizes.length||found.has(id))throw Error('Invalid span marker');
   found.add(id);
   const begin=match.index!, beginEnd=begin+match[0].indexOf('\uE001')+1, end=begin+match[0].length-1;
   return {begin,beginEnd,end,endEnd:end+1,size:sizes[id]};
  });
  let offset=0;
  for(const node of nodes){
   const value=node.data, from=offset, to=offset+value.length;offset=to;
   if(!ranges.some(r=>r.begin<to&&r.endEnd>from))continue;
   const fragment=doc.createDocumentFragment();
   let cursor=from;
   while(cursor<to){
    const range=ranges.find(r=>cursor>=r.begin&&cursor<r.endEnd);
    if(!range){const next=Math.min(to,...ranges.filter(r=>r.begin>cursor).map(r=>r.begin));fragment.appendChild(doc.createTextNode(value.slice(cursor-from,next-from)));cursor=next;continue}
    if(cursor<range.beginEnd){cursor=Math.min(to,range.beginEnd);continue}
    if(cursor>=range.end){cursor=Math.min(to,range.endEnd);continue}
    const end=Math.min(to,range.end);const part=doc.createElementNS('http://www.w3.org/2000/svg','tspan');
    part.setAttribute('font-size',`${range.size}px`);part.setAttribute('style',`font-size:${range.size}px`);
    part.textContent=value.slice(cursor-from,end-from);fragment.appendChild(part);cursor=end;
   }
   node.parentNode!.replaceChild(fragment,node);
  }
 }
 if(found.size!==sizes.length)throw Error('Cannot locate diagram text');
 return new XMLSerializer().serializeToString(doc.documentElement);
}
