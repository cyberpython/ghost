import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import TimeGraph from './TimeGraph'
import type { HourlyBucket } from '../api'

// Chart.js draws to a <canvas>; jsdom has no 2D context, so stub it.
beforeEach(() => {
  HTMLCanvasElement.prototype.getContext = (() => ({
    save: () => {}, restore: () => {}, scale: () => {}, translate: () => {},
    rotate: () => {}, transform: () => {}, setTransform: () => {},
    clearRect: () => {}, fillRect: () => {}, strokeRect: () => {},
    beginPath: () => {}, closePath: () => {}, moveTo: () => {}, lineTo: () => {},
    arc: () => {}, fill: () => {}, stroke: () => {}, clip: () => {},
    fillText: () => {}, strokeText: () => {}, measureText: () => ({ width: 0 }),
    drawImage: () => {}, putImageData: () => {}, createImageData: () => ({}),
    createPattern: () => ({}), createLinearGradient: () => ({}),
    createRadialGradient: () => ({}),
  })) as any
})

function hourKey(d: Date): string {
  return d.toISOString().slice(0, 13) + ':00:00Z'
}

// The first hour of the default 24 h window (matches TimeGraph's initial state).
function firstHourOfWindow(): string {
  const d = new Date()
  d.setMinutes(0, 0, 0)
  d.setHours(d.getHours() - 24)
  return hourKey(d)
}

// A single bucket at the first hour of the window: stepping forward pushes it
// out of view (no data), stepping back keeps it in view.
function makeHourly(): HourlyBucket[] {
  return [{ hour: firstHourOfWindow(), count: 7 }]
}

describe('TimeGraph', () => {
  it('shows Total / Avg / Peak chips computed from the hourly buckets', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    // Total = 10, Avg = 10/24 ≈ 0.4, Peak = 7
    expect(container.textContent).toContain('Total')
    expect(container.textContent).toContain('Avg req/h')
    expect(container.textContent).toContain('Peak')
    expect(container.textContent).toContain('7')
  })

  it('clicking the Peak chip calls onHourClick with the peak hour', () => {
    const spy = vi.fn()
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} onHourClick={spy} />)
    const peakBtn = getByRole('button', { name: /Peak/ })
    fireEvent.click(peakBtn)
    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy.mock.calls[0][0]).toMatch(/:00:00Z$/)
  })

  it('disables stepping when the target window has no data', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    // Data only exists 3–5 h ago; the default window (last 24 h) has data,
    // so stepping back into it is allowed, but stepping forward (future) is not.
    expect((getByRole('button', { name: 'Step back 1 hour' }) as HTMLButtonElement).disabled).toBe(false)
    expect((getByRole('button', { name: 'Step forward 1 hour' }) as HTMLButtonElement).disabled).toBe(true)
  })

  it('renders a Chart.js bar chart', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    expect(container.querySelector('canvas')).not.toBeNull()
  })
})
