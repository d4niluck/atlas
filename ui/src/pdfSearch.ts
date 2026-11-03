/** Locate a search fragment without assuming PDF.js and the index extract identical text. */
export function normalizePdfText(value:string):string {
 return value.normalize('NFKC').toLocaleLowerCase().replace(/[^\p{L}\p{N}]/gu,'');
}
export function pdfAnchors(value:string):string[] {
 const s=normalizePdfText(value);if(s.length<4)return [];
 const n=s.length<12?s.length:Math.min(64,Math.max(12,Math.floor(s.length/3)));
 return [...new Set([s.slice(0,n),s.slice(Math.max(0,Math.floor((s.length-n)/2)),Math.max(0,Math.floor((s.length-n)/2))+n),s.slice(-n)])];
}
type TextItem={str:string;transform:number[];width:number;height:number;hasEOL?:boolean};
export type PdfTextItem=TextItem;
type PdfDocument={numPages:number;getPage:(page:number)=>Promise<{getTextContent:()=>Promise<{items:unknown[]}>}>};
export type PdfMatch={page:number;items:TextItem[];matches:number[];score:number;offset:number;ambiguous:boolean;partial:boolean};
/** Asynchronous per-page extraction; stale requests are cancelled by caller. */
export async function locatePdfFragment(pdf:PdfDocument,text:string,targetOffset:number|null,cancelled:()=>boolean,progress:(page:number,total:number)=>void):Promise<PdfMatch|null> {
 const anchors=pdfAnchors(text);if(!anchors.length)return null;
 const normalized=normalizePdfText(text);const anchorOffsets=anchors.map(a=>normalized.indexOf(a));
 let best:PdfMatch|null=null;let offset=0;let bestPages=0;
 for(let page=1;page<=pdf.numPages;page++){
  if(cancelled())return null;
  const obj=await pdf.getPage(page);if(cancelled())return null;
  const content=await obj.getTextContent();if(cancelled())return null;
  const items=content.items.filter((v):v is TextItem=>typeof (v as TextItem).str==='string');
  const pieces=items.map(item=>normalizePdfText(item.str));const joined=pieces.join('');const positions:number[]=[];
  const pageLength=items.reduce((s,item)=>s+Array.from(item.str).length+(item.hasEOL?1:0),0);
  const preferred=targetOffset===null?0:Math.max(0,targetOffset-offset)*(joined.length/Math.max(1,pageLength));
  const hits=anchors.map((a,i)=>{let best=-1;const expected=preferred+anchorOffsets[i];for(let p=joined.indexOf(a);p>=0;p=joined.indexOf(a,p+1))if(best<0||Math.abs(p-expected)<Math.abs(best-expected))best=p;return best<0?null:[best,best+a.length] as [number,number]});
  // Anchors must preserve order and roughly their original spacing: unrelated
  // common phrases elsewhere on a page do not make a reliable match.
  let coherent:number[]=[];
  for(let mask=1;mask<(1<<anchors.length);mask++){
   const indexes=anchors.map((_,i)=>i).filter(i=>(mask&(1<<i))&&hits[i]);
   if(indexes.length<=coherent.length)continue;
   if(indexes.every((i,k)=>{if(!k)return true;const previous=indexes[k-1],distance=hits[i]![0]-hits[previous]![0],expected=anchorOffsets[i]-anchorOffsets[previous];return distance>=Math.max(0,expected-64)&&distance<=expected+64}))coherent=indexes;
  }
  if(coherent.length===1&&anchors[coherent[0]].length>=32&&new Set(anchors[coherent[0]]).size<6)coherent=[];
  const primary=coherent.length?hits[coherent[0]]:null;
  let at=0;for(let i=0;i<pieces.length;i++){const end=at+pieces[i].length;if(primary&&primary[0]<end&&primary[1]>at)positions.push(i);at=end;}
  const score=coherent.length;const midpoint=offset+pageLength/2;
  const distance=targetOffset===null?offset:Math.abs(midpoint-targetOffset);
  if(score){if(!best||score>best.score)bestPages=1;else if(score===best.score)bestPages++;if(!best||score>best.score||(score===best.score&&distance<best.offset))best={page,items,matches:positions,score,offset:distance,ambiguous:false,partial:score<anchors.length};}
  offset+=pageLength+1;
  if(page%8===0||page===pdf.numPages){progress(page,pdf.numPages);await new Promise<void>(resolve=>setTimeout(resolve,0));}
 }
 // A single short hit in a long fragment is too ambiguous to navigate silently.
 if(best && anchors.length>1 && best.score<2 && anchors[0].length<32)return null;
 if(best){best.ambiguous=bestPages>1;if(best.ambiguous&&targetOffset===null)return null;}
 return best;
}
