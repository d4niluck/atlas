/** Allow a small, validated subset of Mermaid theme configuration and color styles. */
export function prepareMermaid(source:string):{source:string;error:string|null;fontSizes:number[];background:string} {
 const fail = (error:string) => ({source:'',error,fontSizes:[],background:'#ffffff'});
 if (/[\uE000-\uF8FF]/.test(source)) return fail('Недопустимые символы в диаграмме');
 if (!source.trim()) return fail('Диаграмма пуста');
 if (source.length > 12000) return fail('Диаграмма слишком большая');
 if (/^\s*---(?:\r?\n|$)/.test(source)) return fail('Настройки Mermaid внутри диаграммы не поддерживаются');
 let text=source;
 let frontmatter='';
 let background='#ffffff';
 const directive = text.match(/^\s*%%\s*\{\s*init\s*:\s*([\s\S]*?)\}\s*%%/i);
 if (directive) {
  let options:unknown;
  try { options=JSON.parse(directive[1]); } catch { return fail('Некорректные настройки диаграммы'); }
  if (!options || typeof options!=='object' || Array.isArray(options)) return fail('Некорректные настройки диаграммы');
  const config=options as Record<string,unknown>;
  if (Object.keys(config).some(k=>k!=='theme'&&k!=='themeVariables'&&k!=='flowchart')) return fail('Эти настройки диаграммы не поддерживаются');
  if (config.theme!==undefined && config.theme!=='base') return fail('Эта тема диаграммы не поддерживается');
  const flowchart=config.flowchart;
  if (flowchart!==undefined && (!flowchart || typeof flowchart!=='object' || Array.isArray(flowchart))) return fail('Некорректные настройки схемы');
  const spacing=Object.entries((flowchart || {}) as Record<string,unknown>);
  if (spacing.some(([k,v])=>!['nodeSpacing','rankSpacing'].includes(k)||typeof v!=='number'||!Number.isInteger(v)||v<1||v>200)) return fail('Эти настройки схемы не поддерживаются');
  const variables=config.themeVariables;
  if (variables!==undefined && (!variables || typeof variables!=='object' || Array.isArray(variables))) return fail('Некорректные настройки темы');
  const allowed=new Set(['background','primaryTextColor','lineColor','edgeLabelBackground','fontFamily']);
  const entries=Object.entries((variables || {}) as Record<string,unknown>);
  for (const [key,value] of entries) {
   if (!allowed.has(key) || typeof value!=='string') return fail('Эти настройки темы не поддерживаются');
   if (key==='fontFamily' ? value!=='Arial' : !/^#(?:[a-f\d]{3}|[a-f\d]{4}|[a-f\d]{6}|[a-f\d]{8})$/i.test(value)) return fail('Некорректные цвета или шрифт темы');
  }
  const backgroundEntry=entries.find(([key])=>key==='background');
  if (backgroundEntry) background=backgroundEntry[1] as string;
  // Rebuild rather than passing user-supplied configuration to Mermaid.
  frontmatter='---\nconfig:\n  theme: base\n' + (entries.length ? '  themeVariables:\n'+entries.map(([k,v])=>`    ${k}: ${JSON.stringify(v)}`).join('\n')+'\n' : '') + (spacing.length ? '  flowchart:\n'+spacing.map(([k,v])=>`    ${k}: ${v}`).join('\n')+'\n' : '') + '---\n';
  text=text.slice(directive[0].length);
 }
 if (/%%\s*\{/.test(text)) return fail('Настройки Mermaid внутри диаграммы не поддерживаются');
 if (/(?:^|[;\n])\s*(?:style|click|linkStyle)\b/i.test(text)) return fail('Стили и интерактивные действия диаграммы не поддерживаются');
 let invalidClass=false;
 text.replace(/(?:^|[;\n])\s*classDef\s+([^\s;]+)\s+([^;\n]+)/gi, (_match,names:string,styles:string) => {
  if (!/^[\w-]+(?:,[\w-]+)*$/.test(names) || !styles.split(',').every(rule=>/^\s*(?:fill|stroke|color)\s*:\s*#(?:[a-f\d]{3}|[a-f\d]{4}|[a-f\d]{6}|[a-f\d]{8})\s*$/i.test(rule) || /^\s*stroke-width\s*:\s*(?:[0-9]|10)(?:\.\d+)?px\s*$/i.test(rule))) invalidClass=true;
  return _match;
 });
 // Any malformed definition is rejected, not passed through to the renderer.
 const withoutClasses=text.replace(/(?:^|[;\n])\s*classDef\s+([^\s;]+)\s+([^;\n]+)/gi,'');
 if (invalidClass || /(?:^|[;\n])\s*classDef\b/i.test(withoutClasses)) return fail('Некорректные стили диаграммы');
 // Replace a validated span with private markers; SVG formatting is applied after sanitization.
 const fontSizes:number[]=[];
 text=text.replace(/<span\s+style\s*=\s*(['"])font-size\s*:\s*([8-9]|1\d|2\d|3[0-2])px\1\s*>([^<>]*)<\/span\s*>/gi, (_match,_quote,size:string,content:string)=>{
  const id=fontSizes.push(Number(size))-1;
  return `\uE000${id}\uE001${content}\uE002`;
 });
 const withoutBreaks=text.replace(/<br\s*\/?>/gi,'');
 if (/<\s*\/?\s*[A-Za-z]|\b(?:https?:|javascript:|data:|file:)/i.test(withoutBreaks)) return fail('HTML и внешние адреса в диаграммах не поддерживаются');
 return {source:frontmatter+text,error:null,fontSizes,background};
}
export function mermaidInputError(source:string):string|null { return prepareMermaid(source).error; }
