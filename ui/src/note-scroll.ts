// A programmatic scroll emits another scroll event; consume only the expected position.
export function syncScroll(source:HTMLElement,target:HTMLElement,expected:WeakMap<HTMLElement,number>){
 const pending=expected.get(source);expected.delete(source);
 if(pending!==undefined&&Math.abs(source.scrollTop-pending)<1)return;
 const range=source.scrollHeight-source.clientHeight;
 const targetRange=Math.max(0,target.scrollHeight-target.clientHeight);
 const next=range>0?Math.max(0,Math.min(1,source.scrollTop/range))*targetRange:0;
 if(Math.abs(target.scrollTop-next)<1)return;
 expected.set(target,next);target.scrollTop=next;
}
