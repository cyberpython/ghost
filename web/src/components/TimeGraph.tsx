import { useMemo, useState, useCallback } from 'react'
import {
  Chart as ChartJS,
  CategoryScale,
  LinearScale,
  BarElement,
  Tooltip,
  type ChartOptions,
} from 'chart.js'
import { Bar } from 'react-chartjs-2'
import type { HourlyBucket } from '../api'

ChartJS.register(CategoryScale, LinearScale, BarElement, Tooltip)

function hourKey(d: Date): string {
  return d.toISOString().slice(0, 13) + ':00:00Z'
}

interface TimeGraphProps {
  hourly: HourlyBucket[]
  onHourClick: (hour: string) => void
}

export default function TimeGraph({ hourly, onHourClick }: TimeGraphProps) {
  const [windowStart, setWindowStart] = useState<string>(() => {
    const d = new Date()
    d.setMinutes(0, 0, 0)
    d.setHours(d.getHours() - 24)
    return hourKey(d)
  })

  const hours = useMemo(() => {
    const start = new Date(windowStart)
    return Array.from({ length: 24 }, (_, i) => {
      const d = new Date(start)
      d.setHours(start.getHours() + i)
      return hourKey(d)
    })
  }, [windowStart])

  const counts = useMemo(() => {
    const map = new Map(hourly.map(h => [h.hour, h.count]))
    return hours.map(h => map.get(h) ?? null)
  }, [hours, hourly])

  const total = counts.reduce<number>((sum, c) => sum + (c ?? 0), 0)
  const avg = total / 24
  const peak = Math.max(...counts.map(c => c ?? 0))
  const peakIdx = peak > 0 ? counts.findIndex(c => c === peak) : -1
  const peakHour = peakIdx >= 0 ? hours[peakIdx] : null

  const step = useCallback((dir: -1 | 1) => {
    const d = new Date(windowStart)
    d.setHours(d.getHours() + dir)
    setWindowStart(hourKey(d))
  }, [windowStart])

  // Stepping is only allowed when the target 24 h window still contains data,
  // so the graph stops at fully empty windows.
  const { canGoBack, canGoForward } = useMemo(() => {
    const hasData = (startStr: string) => {
      const end = new Date(startStr)
      end.setHours(end.getHours() + 24)
      const endStr = end.toISOString()
      return hourly.some(h => h.hour >= startStr && h.hour < endStr)
    }
    const back = new Date(windowStart); back.setHours(back.getHours() - 1)
    const fwd = new Date(windowStart); fwd.setHours(fwd.getHours() + 1)
    return { canGoBack: hasData(hourKey(back)), canGoForward: hasData(hourKey(fwd)) }
  }, [windowStart, hourly])

  const labels = hours.map(h => h.slice(11, 16))

  const data = {
    labels,
    datasets: [{
      data: counts,
      backgroundColor: '#38bdf8',
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
          title: (items) => hours[items[0].dataIndex],
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
        if (counts[idx] != null) onHourClick(hours[idx])
      }
    },
  }

  return (
    <div className="border border-[var(--border)] rounded p-3">
      <div className="flex flex-wrap gap-2 mb-2">
        <span className="border border-[var(--border)] rounded px-2 py-0.5 text-sm">
          Total <strong>{total}</strong>
        </span>
        <span className="border border-[var(--border)] rounded px-2 py-0.5 text-sm">
          Avg req/h <strong>{avg.toFixed(1)}</strong>
        </span>
        {peakHour && (
          <button
            className="border border-[var(--border)] rounded px-2 py-0.5 text-sm cursor-pointer hover:border-[var(--accent)] transition-colors"
            onClick={() => onHourClick(peakHour)}
            title={`Peak: ${peak} req/h at ${peakHour.slice(11, 16)}`}
          >
            Peak <strong>{peak}</strong> req/h <span className="muted">{peakHour.slice(11, 16)}</span>
          </button>
        )}
      </div>
      <div className="flex items-center gap-2 mb-2">
        <button
          disabled={!canGoBack}
          onClick={() => step(-1)}
          className="border border-[var(--border)] rounded px-2 py-0.5 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          aria-label="Step back 1 hour"
        >
          {'← 1h'}
        </button>
        <button
          disabled={!canGoForward}
          onClick={() => step(1)}
          className="border border-[var(--border)] rounded px-2 py-0.5 text-sm bg-transparent text-[var(--text)] hover:border-[var(--accent)] transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
          aria-label="Step forward 1 hour"
        >
          {'1h →'}
        </button>
        <span className="muted text-sm ml-auto">
          {hours[0].slice(0, 10)} {hours[0].slice(11, 16)} – {hours[23].slice(11, 16)}
        </span>
      </div>
      <div className="h-48">
        <Bar data={data} options={options} />
      </div>
    </div>
  )
}
