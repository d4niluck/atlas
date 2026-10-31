import React from 'react';
import './rag-model-settings.css';

type Model = 'e2b' | 'e4b';
export function RagModelSettings({model, installedE2b, installedE4b, busy, onChange}: {model: Model; installedE2b: boolean; installedE4b: boolean; busy: boolean; onChange: (model: Model) => void}) {
  return <section className="rag-settings" aria-label="Настройки поиска">
    <div className="rag-settings-heading">Модель ответа</div>
    <p className="rag-settings-description">Выберите модель для ответа по найденным источникам.</p>
    <div className="rag-models" role="group" aria-label="Модель ответа">
      {([['e2b', 'Gemma E2B', 'Быстрее', installedE2b], ['e4b', 'Gemma E4B', 'Больше', installedE4b]] as const).map(([id, label, detail, installed]) =>
        <button key={id} type="button" className="rag-model-option" aria-pressed={model === id} disabled={busy || !installed} onClick={() => onChange(id)}>
          <span className="rag-model-copy"><strong>{label}</strong><small>{installed ? detail : 'Не загружена'}</small></span>
          {model === id && <span aria-hidden="true" className="rag-model-check">✓</span>}
        </button>)}
    </div>
  </section>;
}
