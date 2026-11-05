import React from 'react';
import './answer-waiting.css';

/** This indicates an outstanding request, not an inferred model phase. */
export function AnswerWaiting() {
  return <div className="answer-waiting" role="status" aria-live="polite">
    <span>Ищем и готовим ответ</span><span className="answer-waiting-dots" aria-hidden="true"><i/><i/><i/></span>
  </div>;
}
