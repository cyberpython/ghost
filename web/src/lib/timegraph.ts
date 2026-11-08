// Pure window math for the slidable time-graph component (FR-035; reused by
// FR-034/FR-036). Kept out of the component file so both stay fast-refresh
// friendly and the math is unit-testable in isolation.
//
// All arithmetic is pure UTC (`setUTCHours`): bucket keys come from the API as
// UTC hour strings, and local arithmetic would misbehave in DST and
// :30/:45-offset zones (duplicated/missing/shifted hours). `hourKey` only ever
// truncates instants that are already exact UTC hour boundaries.
export function hourKey(d: Date): string {
  return d.toISOString().slice(0, 13) + ':00:00Z'
}

export function shiftHour(iso: string, n: number): string {
  const d = new Date(iso)
  d.setUTCHours(d.getUTCHours() + n)
  return hourKey(d)
}

// 24 consecutive UTC hour buckets starting at `startIso` (inclusive).
export function windowHours(startIso: string): string[] {
  return Array.from({ length: 24 }, (_, i) => shiftHour(startIso, i))
}

// Window anchor derivation for the graph:
// - manual pin wins (the user stepped; new data must not move the window);
// - otherwise follow the most recent capture's hour — the window slides
//   forward automatically as new captures land (a stale/inactive DB still
//   shows the last 24 h of activity that preceded it going quiet);
// - no data at all → wall-clock fallback with the same 24-completed-hours
//   shape (an empty DB has no bars regardless; this keeps the axis sane).
export function windowAnchor(latest: string, manual: string | null, nowIso: string): string {
  if (manual) return manual
  if (latest) return shiftHour(latest, -23)
  const d = new Date(nowIso)
  d.setUTCMinutes(0, 0, 0)
  return shiftHour(hourKey(d), -24)
}

// ── Day-grain helpers (FR-033 chip-driven graph) ────────────────────────────

// UTC day key (YYYY-MM-DD) for a Date.
export function dayKey(d: Date): string {
  return d.toISOString().slice(0, 10)
}

// Shift a UTC day key by n days (can be negative).
export function shiftDay(day: string, n: number): string {
  const d = new Date(day + 'T00:00:00Z')
  d.setUTCDate(d.getUTCDate() + n)
  return dayKey(d)
}

// N consecutive UTC day buckets starting at `startDay` (inclusive).
export function windowDays(startDay: string, count: number): string[] {
  return Array.from({ length: count }, (_, i) => shiftDay(startDay, i))
}

// Day-grain window anchor derivation (mirror of windowAnchor for the 7d chip):
// - manual pin wins;
// - otherwise follow the most recent capture's day — 7 daily buckets ending
//   at that day;
// - no data → wall-clock fallback with the same 7-completed-days shape.
export function dayAnchor(latestDay: string, manual: string | null, nowIso: string): string {
  if (manual) return manual
  if (latestDay) return shiftDay(latestDay, -6)
  const d = new Date(nowIso)
  return shiftDay(dayKey(d), -6)
}

// Span for the "All" window: every UTC day from first to latest (inclusive).
export function daySpan(firstDay: string, latestDay: string): string[] {
  const start = Date.parse(firstDay + 'T00:00:00Z')
  const end = Date.parse(latestDay + 'T00:00:00Z')
  const nDays = Math.round((end - start) / 86_400_000) + 1
  return Array.from({ length: nDays }, (_, i) => shiftDay(firstDay, i))
}

// Sum hourly buckets per UTC day. Returns only days that have captures,
// sorted by day key. Zero-capture days are absent (the component maps them
// to null gaps).
export function rollupByDay(hourly: { hour: string; count: number }[]): { day: string; count: number }[] {
  const m = new Map<string, number>()
  for (const h of hourly) {
    const day = h.hour.slice(0, 10)
    m.set(day, (m.get(day) ?? 0) + h.count)
  }
  return [...m.entries()]
    .sort((a, b) => a[0].localeCompare(b[0]))
    .map(([day, count]) => ({ day, count }))
}

// ── Inclusive upper bounds for click-through filters ────────────────────────

// End of the hour (YYYY-MM-DDTHH:00:00Z) as an inclusive upper bound.
export function endOfHour(hour: string): string {
  const d = new Date(hour)
  d.setUTCMinutes(59, 59)
  return d.toISOString().replace(/\.\d{3}Z$/, 'Z')
}

// End of the day (YYYY-MM-DD) as an inclusive upper bound: YYYY-MM-DDT23:59:59Z.
export function endOfDay(day: string): string {
  const d = new Date(day + 'T00:00:00Z')
  d.setUTCHours(23, 59, 59)
  return d.toISOString().replace(/\.\d{3}Z$/, 'Z')
}
