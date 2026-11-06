export async function protectedSessionUnlocked(id:string, request:typeof fetch=fetch):Promise<boolean>{
 const response=await request('/api/w/'+encodeURIComponent(id)+'/board');
 if(response.ok)return true;
 if(response.status===423||response.status===403)return false;
 throw Error('Не удалось проверить доступ к пространству');
}
export function pendingPasteError(active:Map<string,number>,uncertain:Set<string>):string|null{
 if(uncertain.size)return 'Не удалось подтвердить вставку. Перезапустите Atlas перед выходом.';
 if(active.size)return 'Дождитесь завершения вставки перед выходом.';
 return null;
}
