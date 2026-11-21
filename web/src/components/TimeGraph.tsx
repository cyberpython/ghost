import { useMemo, useState } from 'react'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  BarElement,
  Tooltip,
  type ChartOptions,
} from 'chart.js'
import { Bar } from 'react-chartjs-2'
import type { HourlyBucket, StatWindow } from '../api'
import { useCssVar } from '../lib/useTheme'
import {
  dayAnchor,
  daySpan,
  endOfDay,
  endOfHour,
  rollupByDay,
  shiftDay,
  shiftHour,
  windowAnchor,
  windowDays,
  windowHours,
} from '../lib/timegraph'

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip)

interface TimeGraphProps {
  hourly: HourlyBucket[]
  // FR-033 chip-driven granularity: '24h' → hourly buckets; '7d'/'all' → daily.
  window: StatWindow
  onBucketClick: (since: string, until: string) => void
}

export default function TimeGraph({ hourly, window, onBucketClick }: TimeGraphProps) {
  // Canvas-drawn bars can't use var() directly, so resolve the accent at render
  // time and re-resolve when the theme toggles.
  const barColor = useCssVar('--accent')
  // The window anchors at the most recent capture's bucket (hour or day), not
  // wall-clock now: a stale/inactive DB still shows the activity that preceded
  // it going quiet, and the graph slides forward automatically as new captures
  // land. Stepping pins the window (manual !== null) while investigating; the
  // "Latest" button clears the pin and resumes auto-following. Chip switches
  // remount this component (key in List), which resets the pin — a pin in
  // hour-space is meaningless in day-space.
  const latestHour = hourly.reduce((m, h) => (h.hour > m ? h.hour : m), '')
  // Day keys straight off the bucket strings (no Date parse): first/last UTC
  // day of the filtered set. Empty when there is no data at all. Reducing over
  // a non-empty seed avoids the `'' < x` / `x > ''` sentinel trap that would
  // leave the min side stuck at `''`.
  const dayKeys = hourly.map(h => h.hour.slice(0, 10))
  const firstDay = dayKeys.length ? dayKeys.reduce((a, b) => (a < b ? a : b)) : ''
  const latestDay = dayKeys.length ? dayKeys.reduce((a, b) => (b > a ? b : a)) : ''
  // Mount-clock: only consumed by the empty-DB fallback (pure render); with
  // data present the anchor comes from the buckets, not the wall clock.
  const [bootIso] = useState(() => new Date().toISOString())
  const [manual, setManual] = useState<string | null>(null)

  const isDay = window !== '24h'

  // Bucket keys + per-bucket counts for the active granularity. Missing
  // buckets (zero-capture hours/days) stay null → rendered as gaps, never
  // fabricated zeros.
  const { buckets, counts } = useMemo(() => {
    if (!isDay) {
      const start = windowAnchor(latestHour, manual, bootIso)
      const hours = windowHours(start)
      const map = new Map(hourly.map(h => [h.hour, h.count]))
      return { buckets: hours, counts: hours.map(h => map.get(h) ?? null) }
    }
    let days: string[]
    if (window === '7d') {
      days = windowDays(dayAnchor(latestDay, manual, bootIso), 7)
    } else {
      // All: the entire filtered set, first..latest day. No data → wall-clock
      // shape so the axis stays sane on an empty DB.
      days = latestDay
        ? daySpan(firstDay, latestDay)
        : windowDays(dayAnchor('', null, bootIso), 7)
    }
    const map = new Map(rollupByDay(hourly).map(r => [r.day, r.count]))
    return { buckets: days, counts: days.map(d => map.get(d) ?? null) }
  }, [isDay, window, latestHour, firstDay, latestDay, manual, bootIso, hourly])

  const total = counts.reduce<number>((sum, c) => sum + (c ?? 0), 0)
  const avg = total / buckets.length
  const peak = Math.max(...counts.map(c => c ?? 0))
  const peakIdx = peak > 0 ? counts.findIndex(c => c === peak) : -1
  const peakBucket = peakIdx >= 0 ? buckets[peakIdx] : null

  // Stepping is always available in either direction (accepted FR-035 UX:
  // buttons never disable so empty ranges can't dead-end); "Latest" snaps the
  // window back to the most recent capture's bucket. Both are hidden under All —
  // there is nothing to navigate within "everything".
  const step = (dir: -1 | 1) => setManual(isDay ? shiftDay(buckets[0], dir) : shiftHour(buckets[0], dir))
  const resetToLatest = () => setManual(null)

  // Span-aware click-through bounds: hour bar → the hour's since/until; day
  // bar → the day's T00:00:00Z..T23:59:59Z.
  const bounds = (b: string): [string, string] => isDay
    ? [`${b}T00:00:00Z`, endOfDay(b)]
    : [b, endOfHour(b)]

  const unit = isDay ? 'day' : 'h'
  const totalLabel = window === '24h' ? 'Total 24h' : window === '7d' ? 'Total 7d' : 'Total'
  const labels = buckets.map(b => (isDay ? b : b.slice(11, 16)))

  const data = {
    labels,
    datasets: [{
      data: counts,
      backgroundColor: barColor,
      borderRadius: 2,
    }],
  }

  const options: ChartOptions<'bar'> = {
    responsive: true,
    maintainAspectRatio: false,
    plugins: {
      legend: { display: false },
      tooltip: {
        callbacks: {
          title: (items) => buckets[items[0].dataIndex],
          label: (item) => `${item.raw ?? 0} requests`,
        },
      },
    },
    scales: {
      x: { grid: { display: false } },
      y: { beginAtZero: true, ticks: { precision: 0 } },
    },
    onClick: (_evt, elements) => {
      if (elements.length > 0) {
        const idx = elements[0].index
        if (counts[idx] != null) onBucketClick(...bounds(buckets[idx]))
      }
    },
  }

  return (
    <div className="border border-[var(--border)] rounded p-3">
      <div className="flex flex-wrap gap-2 mb-2">
        <span className="border border-[var(--border)] rounded px-2 py-0.5 text-sm">
          {totalLabel} <strong>{total}</strong>
        </span>
        <span className="border border-[var(--border)] rounded px-2 py-0.5 text-sm">
          Avg req/{unit} <strong>{avg.toFixed(1)}</strong>
        </span>
        {peakBucket && (
          <button
            className="border border-[var(--border)] rounded px-2 py-0.5 text-sm cursor-pointer hover:border-[var(--accent)] transition-colors"
            onClick={() => onBucketClick(...bounds(peakBucket))}
            title={`Peak: ${peak} req/${unit} at ${labels[peakIdx]}`}
          >
            Peak <strong>{peak}</strong> req/{unit} <span className="muted">{labels[peakIdx]}</span>
          </button>
        )}
      </div>
      <div className="flex items-center gap-2 mb-2">
        {window !== 'all' && (
          <>
            <button
              onClick={() => step(-1)}
              className="border border-[var(--border)] rounded px-2 py-0.5 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors"
              aria-label={isDay ? 'Step back 1 day' : 'Step back 1 hour'}
            >
              {'← 1' + unit}
            </button>
            <button
              onClick={() => step(1)}
              className="border border-[var(--border)] rounded px-2 py-0.5 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors"
              aria-label={isDay ? 'Step forward 1 day' : 'Step forward 1 hour'}
            >
              {'1' + unit + ' →'}
            </button>
            <button
              onClick={resetToLatest}
              className="border border-[var(--border)] rounded px-2 py-0.5 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors"
              aria-label="Reset to latest capture window"
              title={`Reset the window to the ${window === '24h' ? '24 h' : '7 days'} ending at the most recent capture`}
            >
              <span className="inline-flex items-center gap-1">
                <svg className="w-3.5 h-3.5" fill="none" viewBox="0 0 16 16" stroke="currentColor" strokeWidth={1.5} aria-hidden="true">
                  <circle cx="8" cy="8" r="6.25" />
                  <path d="M8 4.75V8l2.25 1.5" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
                Latest
              </span>
            </button>
          </>
        )}
        <span className="muted text-sm ml-auto">
          {isDay
            ? `${buckets[0]} – ${buckets[buckets.length - 1]}`
            : `${buckets[0].slice(0, 10)} ${buckets[0].slice(11, 16)} – ${buckets[buckets.length - 1].slice(11, 16)}`}
        </span>
      </div>
      <div className="h-48">
        <Bar data={data} options={options} />
      </div>
    </div>
  )
}
