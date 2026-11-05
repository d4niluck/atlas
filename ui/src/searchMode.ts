export type SearchMode = 'lexical'|'semantic'|'hybrid'|'rag';
export type SimpleSearchMode = Exclude<SearchMode,'rag'>;
export function selectSearchMode(current:SearchMode, previous:SimpleSearchMode, selected:'lexical'|'semantic'):SimpleSearchMode {
 if(current==='rag') return previous==='hybrid'?'hybrid':selected;
 if(current==='hybrid') return selected==='lexical'?'semantic':'lexical';
 return current===selected?current:'hybrid';
}
