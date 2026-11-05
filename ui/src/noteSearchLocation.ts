import type {SearchChunk} from './SearchResults';

// Search offsets refer to the saved text; an open editor may contain unsaved changes.
export function locateNoteChunk(text:string, chunk:SearchChunk):SearchChunk|null {
 if(chunk.start==null||chunk.end==null||!chunk.text)return chunk;
 const original=Array.from(text).slice(chunk.start,chunk.end).join('');
 if(original.startsWith(chunk.text))return chunk;
 const index=text.indexOf(chunk.text);
 if(index<0||text.lastIndexOf(chunk.text)!==index)return null;
 const start=Array.from(text.slice(0,index)).length;
 return {...chunk,start,end:start+Array.from(chunk.text).length};
}
