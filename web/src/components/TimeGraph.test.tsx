import { describe, it, expect, vi } from 'vitest'
import { render, fireEvent } from '@testing-library/react'
import TimeGraph from './TimeGraph'
import { windowHours, windowAnchor } from '../lib/timegraph'
import type { HourlyBucket } from '../api'

// Chart.js's responsive/DOM machinery cannot run under jsdom (it dereferences
// canvas.parentNode during reactive updates); swap Bar for a plain canvas so
// the component's window logic is testable. Chart.js internal wiring (tooltips,
// bar click-through) is verified in-browser.
vi.mock('react-chartjs-2', async () => {
  const React = await import('react')
  return { Bar: () => React.createElement('canvas') }
})

// The most recent capture in every fixture: a data-anchored hour in the past,
// distinct from the test runner's wall clock on purpose.
const LATEST = '2024-01-05T20:00:00Z'
function makeHourly(hour: string = LATEST): HourlyBucket[] {
  return [{ hour, count: 7 }]
}

const caption = (c: HTMLElement): string =>
  (Array.from(c.querySelectorAll('.ml-auto')).at(-1)?.textContent ?? '')

describe('TimeGraph', () => {
  it('shows Total / Avg / Peak chips computed from the hourly buckets', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    expect(container.textContent).toContain('Total 24h')
    expect(container.textContent).toContain('Avg req/h')
    expect(container.textContent).toContain('Peak')
    expect(container.textContent).toContain('7')
  })

  it('anchors the default window at the most recent capture, not wall-clock now', () => {
    // start = latest − 23 h → caption "2024-01-04 21:00 – 20:00"
    const { container } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    expect(caption(container)).toContain('2024-01-04 21:00')
    expect(caption(container)).toContain('20:00')
  })

  it('stepping pins the window and the Latest button re-anchors it', () => {
    const v = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    fireEvent.click(v.getByRole('button', { name: 'Step back 1 hour' }))
    // pinned one hour back: 2024-01-04 20:00 – 19:00
    expect(caption(v.container)).toContain('2024-01-04 20:00')
    fireEvent.click(v.getByRole('button', { name: 'Reset to latest capture window' }))
    // back to the fresh data anchor (2024-01-04 21:00 – 20:00)
    expect(caption(v.container)).toContain('2024-01-04 21:00')
  })

  it('follow/pin semantics (pure derivation): pins survive newer captures', () => {
    const now = '2024-01-05T21:30:00Z'
    // follow: anchor at the latest capture's hour, minus 23 h
    expect(windowAnchor('2024-01-05T20:00:00Z', null, now)).toBe('2024-01-04T21:00:00Z')
    // newer capture lands — follow mode slides the window forward one hour
    expect(windowAnchor('2024-01-05T21:00:00Z', null, now)).toBe('2024-01-04T22:00:00Z')
    // a stepped window stays put when newer data arrives
    expect(windowAnchor('2024-01-05T21:00:00Z', '2024-01-04T20:00:00Z', now)).toBe('2024-01-04T20:00:00Z')
  })

  it('clicking the Peak chip calls onHourClick with the peak hour', () => {
    const spy = vi.fn()
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} onHourClick={spy} />)
    const peakBtn = getByRole('button', { name: /Peak/ })
    fireEvent.click(peakBtn)
    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy.mock.calls[0][0]).toMatch(/:00:00Z$/)
  })

  it('keeps stepping enabled even when the target window has no data', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    // Accepted UX: never dead-end into disabled buttons; "Latest" is the
    // escape hatch back to the freshest data window.
    expect((getByRole('button', { name: 'Step back 1 hour' }) as HTMLButtonElement).disabled).toBe(false)
    expect((getByRole('button', { name: 'Step forward 1 hour' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('provides a Latest button that re-anchors at the most recent capture', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    const btn = getByRole('button', { name: 'Reset to latest capture window' })
    expect(btn).toBeTruthy()
    expect((btn as HTMLButtonElement).disabled).toBe(false)
    expect(btn.getAttribute('title')).toMatch(/most recent capture/)
  })

  it('renders a Chart.js bar chart', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} onHourClick={() => {}} />)
    expect(container.querySelector('canvas')).not.toBeNull()
  })

  it('windowHours yields 24 distinct consecutive UTC hour buckets', () => {
    // Anchored exactly on America/New_York's 2026 spring-forward instant,
    // where local hour arithmetic used to duplicate/drop buckets.
    const h = windowHours('2026-03-08T06:00:00Z')
    expect(h).toHaveLength(24)
    expect(new Set(h).size).toBe(24)
    expect(h[0]).toBe('2026-03-08T06:00:00Z')
    expect(h[23]).toBe('2026-03-09T05:00:00Z')
    const asInts = h.map(x => Date.parse(x))
    for (let i = 1; i < asInts.length; i++) expect(asInts[i] - asInts[i - 1]).toBe(3600_000)
  })
})
