export type NoteMode={expanded:boolean;editing:boolean};
export function nextNoteMode(current:NoteMode,action:'expand'|'toggle-edit'):NoteMode{
 if(action==='toggle-edit')return {...current,editing:!current.editing};
 return {expanded:!current.expanded,editing:!current.expanded||current.editing};
}
