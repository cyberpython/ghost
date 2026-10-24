// Human-readable byte sizes for table cells: integer bytes; one decimal
// below 10 of the scaled unit, none above (rounding keeps cells compact).
// GB is the top tier — HTTP bodies don't reach TB; clamp non-finite/negative
// inputs to 0 B defensively in case the helper is reused elsewhere.
export function formatSize(n: number): string {
  if (!Number.isFinite(n) || n <= 0) return '0 B'
  if (n < 1024) return `${n} B`
  const kb = n / 1024
  if (kb < 1024) return `${kb < 10 ? kb.toFixed(1) : Math.round(kb)} KB`
  const mb = kb / 1024
  if (mb < 1024) return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`
  const gb = mb / 1024
  return `${gb < 10 ? gb.toFixed(1) : Math.round(gb)} GB`
}
