export function supportsFileDescription(title:string,status?:string,highlightField?:string) {
 return /\.pdf$/i.test(title)||status==='unavailable'||status==='unsupported'||highlightField==='description';
}
