import type {SearchSettingsValue} from './SearchSettings';
type Handle={prepare:()=>Promise<void>;cancel:()=>void};
export async function applySearchSettings(handles:Handle[], settings:SearchSettingsValue, prepareRestart:()=>Promise<unknown>, save:(value:SearchSettingsValue)=>Promise<void>, restart:()=>Promise<unknown>) {
 const prepared:Handle[]=[];
 try {for(const handle of handles){await handle.prepare();prepared.push(handle)}await prepareRestart();await save(settings)}
 catch(error){for(const handle of prepared.reverse())handle.cancel();throw error}
 try {await restart()}
 catch {for(const handle of prepared.reverse())handle.cancel();throw Error('Настройки сохранены, но перезапустить Atlas не удалось. Закройте и откройте приложение вручную; сервер мог остановиться.')}
}
