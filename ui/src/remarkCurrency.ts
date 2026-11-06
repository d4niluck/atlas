// A pair of prices ($5 and $10) is not an inline mathematical expression.
// Only recover the unambiguous numeric-price pattern; all other math is left intact.
type Node={type:string;value?:string;children?:Node[];position?:{start:{offset?:number};end:{offset?:number}}};
export function remarkCurrency(){return (tree:Node,file:{value:unknown})=>{
 const source=typeof file.value==='string'?file.value:'';
 const firstText=(node:Node|undefined):string=>node?.type==='text'?(node.value??''):node?.children?.length?firstText(node.children[0]):'';
 const visit=(parent:Node)=>{const children=parent.children;if(!children)return;
  children.forEach((node,index)=>{const next=children[index+1];
   if(node.type==='inlineMath'&&/^[+-]?\d/.test(node.value??'')&&/^\d/.test(firstText(next))){
    const start=node.position?.start.offset,end=node.position?.end.offset;
    const literal=start!==undefined&&end!==undefined?source.slice(start,end):`$${node.value??''}$`;
    children[index]={type:'text',value:literal,position:node.position};
   }else visit(node);
  });
 };visit(tree);
};}
