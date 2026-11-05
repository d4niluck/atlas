/** Preserve the original source numbers used by the model, including gaps. */
export function citedSources<T>(answer: string | null | undefined, sources: T[]): {number: number; source: T}[] {
  const cited = new Set<number>();
  for (const match of (answer ?? '').matchAll(/\[(\d+)\]/g)) {
    const number = Number(match[1]);
    if (Number.isSafeInteger(number) && number >= 1 && number <= sources.length) cited.add(number);
  }
  return sources.flatMap((source, index) => cited.has(index + 1) ? [{number: index + 1, source}] : []);
}
