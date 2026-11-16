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
type PdfDocument={numPages:number;getPage:(page:number)=>Promise<{isPureXfa?:boolean;streamTextContent:()=>ReadableStream<{items:unknown[]}>}>};
export type PdfMatch={page:number;items:TextItem[];matches:number[];score:number;offset:number;ambiguous:boolean;partial:boolean};
/** Asynchronous per-page extraction; stale requests are cancelled by caller. */
export async function locatePdfFragment(pdf:PdfDocument,text:string,targetOffset:number|null,cancelled:()=>boolean,progress:(page:number,total:number)=>void,onPage?:(page:number,start:number,end:number,normalized:string)=>void):Promise<PdfMatch|null> {
 const anchors=pdfAnchors(text);if(!anchors.length)return null;
 const normalized=normalizePdfText(text);const anchorOffsets=anchors.map(a=>normalized.indexOf(a));
 let best:PdfMatch|null=null;let offset=0;let bestPages=0;
 for(let page=1;page<=pdf.numPages;page++){
  if(cancelled())return null;
  const obj=await pdf.getPage(page);if(cancelled())return null;
  // XFA forms do not provide PDF text geometry; never guess a destination.
  if(obj.isPureXfa)return null;
  // Older macOS WKWebView does not provide ReadableStream async iteration,
  // which PDF.js getTextContent() uses internally. Read via the standard reader.
  const reader=obj.streamTextContent().getReader();const items:TextItem[]=[];let completed=false;
  try {while(!cancelled()){const part=await reader.read();if(part.done){completed=true;break}for(const value of part.value.items)if(typeof (value as TextItem).str==='string')items.push(value as TextItem)}}
  finally {if(!completed)try{await reader.cancel()}catch{}reader.releaseLock()}
  if(cancelled())return null;
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
  onPage?.(page,offset,offset+pageLength,joined);
  offset+=pageLength+1;
  if(page%8===0||page===pdf.numPages){progress(page,pdf.numPages);await new Promise<void>(resolve=>setTimeout(resolve,0));}
 }
 // A single short hit in a long fragment is too ambiguous to navigate silently.
 if(best && anchors.length>1 && best.score<2 && anchors[0].length<32)return null;
 if(best){best.ambiguous=bestPages>1;if(best.ambiguous&&targetOffset===null)return null;}
 return best;
}

/** Keep a small cache only while this PDF is open; different offsets remain distinct. */
export function createPdfFragmentLocator(pdf:PdfDocument) {
 const cache=new Map<string,PdfMatch>();
 let pageOffsets:{start:number;end:number;normalized:string}[]=[];
 return async (text:string,offset:number|null,cancelled:()=>boolean,progress:(page:number,total:number)=>void)=>{
  const key=JSON.stringify([text,offset]);const cached=cache.get(key);
  if(cached&&!cancelled())return cached;
  // Text offsets from the index are approximate; accept a direct page only
  // when every anchor matches, otherwise fall back to the complete scan.
  const guess=offset===null?-1:pageOffsets.findIndex(p=>offset>=p.start&&offset<=p.end);
  const anchors=pdfAnchors(text);
  const unique=anchors.length>0&&pageOffsets.every((p,index)=>index===guess||!anchors.every(anchor=>p.normalized.includes(anchor)));
  if(guess>=0&&unique){const page=guess+1;const single={numPages:1,getPage:()=>pdf.getPage(page)};
   const found=await locatePdfFragment(single,text,offset! - pageOffsets[guess].start,cancelled,()=>{});
   if(found&&!found.partial&&!cancelled()){const direct={...found,page};cache.set(key,direct);if(cache.size>64)cache.delete(cache.keys().next().value!);return direct}
  }
  const offsets:{start:number;end:number;normalized:string}[]=[];let storedBytes=0;let cacheable=true;
  const found=await locatePdfFragment(pdf,text,offset,cancelled,progress,(page,start,end,normalized)=>{storedBytes+=normalized.length*2;if(storedBytes>8*1024*1024){cacheable=false;offsets.length=0}else if(cacheable)offsets[page-1]={start,end,normalized}});
  if(!cancelled()&&cacheable&&offsets.length===pdf.numPages)pageOffsets=offsets;
  if(found&&!cancelled()){if(cache.size>=64)cache.delete(cache.keys().next().value!);cache.set(key,found)}
  return found;
 };
}
