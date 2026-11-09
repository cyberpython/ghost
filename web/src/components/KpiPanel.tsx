import { useMemo } from 'react'
import type { MethodStatusCell, StatWindow } from '../api'
import { methodColor, statusClassColor } from '../lib/colors'

const STATUS_CLASSES = ['2xx', '3xx', '4xx', '5xx', 'err'] as const
type StatusClass = (typeof STATUS_CLASSES)[number]

// FR-033 selectable windows: 24 h maps to the same 24 hourly buckets the
// requests/hour graph plots, so the panel's numbers agree with the bars.
const WINDOW_OPTIONS: { k: StatWindow; label: string }[] = [
  { k: '24h', label: '24 h' },
  { k: '7d', label: '7 d' },
  { k: 'all', label: 'All' },
]

interface KpiPanelProps {
  byMethodStatus: MethodStatusCell[]
  total: number
  statWindow: StatWindow
  onStatWindow: (w: StatWindow) => void
}

export default function KpiPanel({ byMethodStatus, total, statWindow, onStatWindow }: KpiPanelProps) {
  const { methods, grid } = useMemo(() => {
    const methods = new Set<string>()
    const grid = new Map<string, number>()
    for (const cell of byMethodStatus) {
      methods.add(cell.method)
      grid.set(`${cell.method}|${cell.status_class}`, cell.count)
    }
    return { methods: [...methods].sort(), grid }
  }, [byMethodStatus])

  const methodTotals = useMemo(() => {
    const m = new Map<string, number>()
    for (const cell of byMethodStatus) {
      m.set(cell.method, (m.get(cell.method) ?? 0) + cell.count)
    }
    return m
  }, [byMethodStatus])

  const classTotals = useMemo(() => {
    const m = new Map<StatusClass, number>()
    for (const cell of byMethodStatus) {
      m.set(cell.status_class as StatusClass, (m.get(cell.status_class as StatusClass) ?? 0) + cell.count)
    }
    return m
  }, [byMethodStatus])

  // Windowed views show the sum of the breakdown cells (its own total); the
  // 'all' view shows the endpoint's unfiltered total.
  const cellsSum = byMethodStatus.reduce((s, c) => s + c.count, 0)
  const shownTotal = statWindow === 'all' ? total : cellsSum

  return (
    <div className="border border-[var(--border)] rounded p-3">
      <div className="flex items-center flex-wrap gap-2 mb-2">
        <h2 className="text-sm font-medium">Traffic by method × status class</h2>
        <span className="muted text-sm">total {shownTotal}</span>
        <div className="flex items-center gap-1 ml-auto">
          {WINDOW_OPTIONS.map(({ k, label }) => (
            <button key={k} onClick={() => onStatWindow(k)}
              className={k === statWindow
                ? 'px-2 py-0.5 text-xs rounded bg-[var(--panel)] border border-[var(--border)] cursor-pointer'
                : 'px-2 py-0.5 text-xs rounded border border-[var(--border)] cursor-pointer hover:border-[var(--accent)] transition-colors'}
              title={k === '24h'
                ? 'Last 24 hourly buckets ending at the most recent capture'
                : k === '7d'
                  ? 'Last 168 hourly buckets ending at the most recent capture'
                  : 'Active filters, entire history'}>
              {label}
            </button>
          ))}
        </div>
      </div>
      {methods.length === 0 ? (
        <p className="muted text-sm">No captures match the active filters.</p>
      ) : (
        <table className="border-collapse w-full">
          <thead>
            <tr>
              <th className="text-left px-2 py-1 text-sm font-medium">Method</th>
              {STATUS_CLASSES.map(sc => (
                <th key={sc} className="text-right px-2 py-1 text-sm font-medium" style={{ color: statusClassColor(sc) }}>
                  {sc}
                </th>
              ))}
              <th className="text-right px-2 py-1 text-sm font-medium">Total</th>
            </tr>
          </thead>
          <tbody>
            {methods.map(m => (
              <tr key={m} className="border-t border-[var(--border)]">
                <td className="px-2 py-1 text-sm font-medium" style={{ color: methodColor(m) }}>{m}</td>
                {STATUS_CLASSES.map(sc => {
                  const v = grid.get(`${m}|${sc}`)
                  return (
                    <td key={sc} className="px-2 py-1 text-right tabular-nums text-sm"
                      style={v ? { color: statusClassColor(sc) } : { color: 'var(--muted)' }}>
                      {v ?? '·'}
                    </td>
                  )
                })}
                <td className="px-2 py-1 text-right tabular-nums text-sm font-medium">
                  {methodTotals.get(m) ?? 0}
                </td>
              </tr>
            ))}
          </tbody>
          <tfoot>
            <tr className="border-t border-[var(--border)]">
              <td className="px-2 py-1 text-sm font-medium">Total</td>
              {STATUS_CLASSES.map(sc => (
                <td key={sc} className="px-2 py-1 text-right tabular-nums text-sm font-medium"
                  style={{ color: statusClassColor(sc) }}>
                  {classTotals.get(sc) ?? 0}
                </td>
              ))}
              <td className="px-2 py-1 text-right tabular-nums text-sm font-medium">{shownTotal}</td>
            </tr>
          </tfoot>
        </table>
      )}
    </div>
  )
}
