/** Keep an existing query when speech finishes after the user has typed. */
export function appendVoiceText(query:string,text:string):string {
 return query+(query&&!/\s$/.test(query)?' ':'')+text;
}
