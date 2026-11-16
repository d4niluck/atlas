/** Copy in WebKit even when the async Clipboard API is unavailable. */
export async function copyText(text:string):Promise<void>{
 if(navigator.clipboard?.writeText){try{await navigator.clipboard.writeText(text);return}catch{/* Try the legacy WebKit path. */}}
 const input=document.createElement('textarea');input.value=text;input.setAttribute('readonly','');input.style.position='fixed';input.style.opacity='0';document.body.appendChild(input);
 try{input.select();if(!document.execCommand('copy'))throw Error('Copy failed')}finally{input.remove()}
}
