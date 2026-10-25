import { useMemo } from 'react'
import type { MethodStatusCell } from '../api'
import { methodColor } from '../lib/colors'

const STATUS_CLASSES = ['2xx', '3xx', '4xx', '5xx', 'err'] as const
type StatusClass = (typeof STATUS_CLASSES)[number]

const STATUS_CLASS_COLOR: Record<StatusClass, string> = {
  '2xx': '#22c55e',
  '3xx': '#38bdf8',
  '4xx': '#f59e0b',
  '5xx': '#ef4444',
  'err': '#a78bfa',
}

interface KpiPanelProps {
  byMethodStatus: MethodStatusCell[]
  total: number
}

export default function KpiPanel({ byMethodStatus, total }: KpiPanelProps) {
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

  return (
    <div className="border border-[var(--border)] rounded p-3">
      <div className="flex items-baseline gap-2 mb-2">
        <h2 className="text-sm font-medium">Traffic by method × status class</h2>
        <span className="muted text-sm">total {total}</span>
      </div>
      {methods.length === 0 ? (
        <p className="muted text-sm">No captures match the active filters.</p>
      ) : (
        <table className="border-collapse w-full">
          <thead>
            <tr>
              <th className="text-left px-2 py-1 text-sm font-medium">Method</th>
              {STATUS_CLASSES.map(sc => (
                <th key={sc} className="text-right px-2 py-1 text-sm font-medium" style={{ color: STATUS_CLASS_COLOR[sc] }}>
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
                      style={v ? { color: STATUS_CLASS_COLOR[sc] } : { color: 'var(--muted)' }}>
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
                  style={{ color: STATUS_CLASS_COLOR[sc] }}>
                  {classTotals.get(sc) ?? 0}
                </td>
              ))}
              <td className="px-2 py-1 text-right tabular-nums text-sm font-medium">{total}</td>
            </tr>
          </tfoot>
        </table>
      )}
    </div>
  )
}
