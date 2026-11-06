// Avoid blocking the UI on unusually large Markdown code blocks.
export async function highlightPython(source:string):Promise<string|null> {
 if(source.length>200_000) return null;
 const [{default:hljs},{default:python}]=await Promise.all([
  import('highlight.js/lib/core'),import('highlight.js/lib/languages/python')]);
 if(!hljs.getLanguage('python')) hljs.registerLanguage('python',python);
 return hljs.highlight(source,{language:'python',ignoreIllegals:true}).value;
}
