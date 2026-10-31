import assert from 'node:assert/strict';
import {test} from 'node:test';
import {mermaidInputError} from '../src/mermaid-safety.ts';
test('обычные flowchart и sequence разрешены',()=>{
 assert.equal(mermaidInputError('flowchart LR\n A[Начало] --> B[Конец]'),null);
 assert.equal(mermaidInputError('sequenceDiagram\n Alice->>Bob: Привет'),null);
});
test('конфигурация, код и ссылки не исполняются',()=>{
 for(const text of ['%%{init: {"securityLevel":"loose"}}%%\nflowchart LR\n A-->B', '---\nconfig: {}\n---\nflowchart LR\n A-->B', 'flowchart LR\n A-->B;click A "https://example.com"', 'flowchart LR\n A[<script>alert(1)</script>]', 'flowchart LR\n A-->B;style A fill:red']) assert.ok(mermaidInputError(text));
});
test('допускаются безопасные цвета темы, classDef и br',()=>{
 const text='%%{init: {"theme":"base","themeVariables":{"background":"#111827","primaryTextColor":"#FFFFFF","lineColor":"#94A3B8","edgeLabelBackground":"#111827","fontFamily":"Arial"}}}%%\nflowchart LR\n A["Начало<br/>Конец"] --> B\n classDef sample fill:#1E293B,stroke:#64748B,color:#FFFFFF';
 assert.equal(mermaidInputError(text),null);
});
test('недопустимые настройки, стиль и разметка отклоняются',()=>{
 for(const text of ['%%{init: {"securityLevel":"loose"}}%%\nflowchart LR\n A-->B','flowchart LR\n classDef x fill:url(https://evil.example)','flowchart LR\n classDef x fill:#fff,color:red','flowchart LR\n A["<br onerror=alert(1)>"]']) assert.ok(mermaidInputError(text));
});
test('безопасный font-size сохраняется как данные, а не HTML', async()=>{
 const {prepareMermaid}=await import('../src/mermaid-safety.ts');
 const parsed=prepareMermaid('flowchart LR\n A["Верх<br/><span style=\'font-size:12px\'>Подпись</span>"]');
 assert.equal(parsed.error,null);assert.deepEqual(parsed.fontSizes,[12]);
 assert.equal(parsed.source.includes('<span'),false);assert.ok(parsed.source.includes('\uE0000\uE001Подпись\uE002'));
});
test('фон темы сохраняется только как проверенный шестнадцатеричный цвет', async()=>{
 const {prepareMermaid}=await import('../src/mermaid-safety.ts');
 const text='%%{init: {"theme":"base","themeVariables":{"background":"#111827"}}}%%\nflowchart LR\n A-->B';
 const parsed=prepareMermaid(text);assert.equal(parsed.error,null);assert.equal(parsed.background,'#111827');
});
