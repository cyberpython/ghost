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
