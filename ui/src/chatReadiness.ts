type RagStatus = {available:boolean;message:string;model?:'e2b'|'e4b'|'nemotron';installed_e2b?:boolean;installed_e4b?:boolean;installed_nemotron?:boolean};

export function chatReadiness(settings:{semantic_enabled:boolean;rag_enabled:boolean},rag:RagStatus,semanticMessage:string):string|null {
 if(!settings.semantic_enabled)return 'Для чата включите смысловой и умный поиск в настройках.';
 if(!settings.rag_enabled)return 'Для чата включите умный поиск в настройках.';
 const installed=rag.model&&({e2b:rag.installed_e2b,e4b:rag.installed_e4b,nemotron:rag.installed_nemotron})[rag.model];
 if(rag.model&&installed===false)return 'Загрузите выбранную модель ответа в настройках поиска.';
 if(semanticMessage==='Модель не загружена')return 'Загрузите модель смыслового поиска в настройках.';
 if(!rag.available)return rag.message||'Модель ответа пока недоступна. Проверьте настройки поиска.';
 return null;
}
