import React from 'react';
import './agentic-toggle.css';

export function AgenticToggle({enabled, disabled, onChange}: {enabled:boolean; disabled:boolean; onChange:(enabled:boolean)=>void}) {
  return <label className={'agentic-toggle'+(disabled?' is-disabled':'')}>
    <input type="checkbox" checked={enabled} disabled={disabled} onChange={event=>onChange(event.currentTarget.checked)}/>
    <span><strong>Агентский поиск</strong><small>Разбивает сложный вопрос на подвопросы и ищет информацию поэтапно.</small></span>
  </label>;
}
