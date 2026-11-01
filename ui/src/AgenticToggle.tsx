import React from 'react';
import './agentic-toggle.css';

export function AgenticToggle({enabled, disabled, onChange}: {enabled:boolean; disabled:boolean; onChange:(enabled:boolean)=>void}) {
  return <div className={'agentic-toggle'+(disabled?' is-disabled':'')}>
    <span><strong>Агентский поиск</strong><small>Разбивает сложный вопрос на подвопросы и ищет информацию поэтапно.</small></span>
    <button type="button" role="switch" aria-label="Агентский поиск" aria-checked={enabled} disabled={disabled} className="search-settings-switch" onClick={()=>onChange(!enabled)}><span/></button>
  </div>;
}
