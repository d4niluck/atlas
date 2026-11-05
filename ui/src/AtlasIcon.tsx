import React from 'react';
import './atlas-icon.css';

export function AtlasIcon({large=false}:{large?:boolean}) {
 return <span className={'atlas-icon'+(large?' atlas-icon-large':'')} aria-hidden="true">
  <svg viewBox="0 0 32 32" width="100%" height="100%" focusable="false">
   <circle cx="16" cy="16" r="12" fill="currentColor"/>
   <path d="M9 11.5c1.1-3 4.1-5 7.4-5" fill="none" stroke="#fff" strokeWidth="2" strokeLinecap="round" opacity=".7"/>
   <path d="M7.1 20.5c2.4 3.9 7.9 5.7 12.2 4" fill="none" stroke="#493675" strokeWidth="1.3" strokeLinecap="round" opacity=".45"/>
  </svg>
 </span>;
}
