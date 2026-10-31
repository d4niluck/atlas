export function historyAgentic(mode: string, agentic?: boolean | null, error?: string | null) {
  return {
    agentic: mode === 'rag' && agentic === true,
    error: error || (mode === 'rag' && agentic == null ? 'Режим исходного запроса неизвестен.' : ''),
  };
}
