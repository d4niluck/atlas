// KaTeX changes source characters into a rendered tree; highlight the formula as a whole.
export function markMathSearch<T extends {type:string;position?:{start:{offset?:number};end:{offset?:number}}}>(node:T,from:number,to:number){
 const a=node.position?.start.offset,b=node.position?.end.offset;
 if((node.type==='math'||node.type==='inlineMath')&&typeof a==='number'&&typeof b==='number'&&to>a&&from<b)
  return {type:'strong',data:{hName:'mark'},children:[node]};
 return null;
}
