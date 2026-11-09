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

// Multi-day fixture for rollup / gap-day tests.
const MULTI: HourlyBucket[] = [
  { hour: '2024-01-01T10:00:00Z', count: 3 },
  { hour: '2024-01-01T11:00:00Z', count: 5 }, // same day → sums to 8
  { hour: '2024-01-03T09:00:00Z', count: 2 }, // gap day 01-02 absent
]

const caption = (c: HTMLElement): string =>
  (Array.from(c.querySelectorAll('.ml-auto')).at(-1)?.textContent ?? '')

// ── 24h (hourly) mode ────────────────────────────────────────────────────────

describe('TimeGraph 24h', () => {
  it('shows Total / Avg / Peak chips computed from the hourly buckets', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
    expect(container.textContent).toContain('Total 24h')
    expect(container.textContent).toContain('Avg req/h')
    expect(container.textContent).toContain('Peak')
    expect(container.textContent).toContain('7')
  })

  it('anchors the default window at the most recent capture, not wall-clock now', () => {
    // start = latest − 23 h → caption "2024-01-04 21:00 – 20:00"
    const { container } = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
    expect(caption(container)).toContain('2024-01-04 21:00')
    expect(caption(container)).toContain('20:00')
  })

  it('stepping pins the window and the Latest button re-anchors it', () => {
    const v = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
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

  it('clicking the Peak chip calls onBucketClick with hour bounds', () => {
    const spy = vi.fn()
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={spy} />)
    fireEvent.click(getByRole('button', { name: /Peak/ }))
    expect(spy).toHaveBeenCalledTimes(1)
    // hour bounds: since = the hour, until = endOfHour
    expect(spy.mock.calls[0][0]).toBe('2024-01-05T20:00:00Z')
    expect(spy.mock.calls[0][1]).toBe('2024-01-05T20:59:59Z')
  })

  it('keeps stepping enabled even when the target window has no data', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
    // Accepted UX: never dead-end into disabled buttons; "Latest" is the
    // escape hatch back to the freshest data window.
    expect((getByRole('button', { name: 'Step back 1 hour' }) as HTMLButtonElement).disabled).toBe(false)
    expect((getByRole('button', { name: 'Step forward 1 hour' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('provides a Latest button that re-anchors at the most recent capture', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
    const btn = getByRole('button', { name: 'Reset to latest capture window' })
    expect(btn).toBeTruthy()
    expect((btn as HTMLButtonElement).disabled).toBe(false)
    expect(btn.getAttribute('title')).toMatch(/most recent capture/)
  })

  it('renders a Chart.js bar chart', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
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

// ── 7d (daily) mode ───────────────────────────────────────────────────────────

describe('TimeGraph 7d', () => {
  it('shows Total 7d / Avg req/day / Peak req/day chips', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={() => {}} />)
    expect(container.textContent).toContain('Total 7d')
    expect(container.textContent).toContain('Avg req/day')
    expect(container.textContent).toContain('req/day')
  })

  it('anchors the 7d window at the most recent capture\'s day (−6 days)', () => {
    // latestDay = 2024-01-05; start = shiftDay(2024-01-05, -6) = 2023-12-30
    const { container } = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={() => {}} />)
    expect(caption(container)).toContain('2023-12-30')
    expect(caption(container)).toContain('2024-01-05')
  })

  it('sums hourly buckets per UTC day; gap days stay null', () => {
    const { container } = render(<TimeGraph hourly={MULTI} window="7d" onBucketClick={() => {}} />)
    // latestDay = 2024-01-03; start = shiftDay(2024-01-03, -6) = 2023-12-28
    expect(caption(container)).toContain('2023-12-28')
    // Total = 8 (Jan 1: 3+5) + 2 (Jan 3) = 10; gap day Jan 2 contributes 0
    expect(container.textContent).toContain('Total 7d')
    expect(container.textContent).toContain('10')
    // Avg = 10/7 ≈ 1.4
    expect(container.textContent).toContain('1.4')
  })

  it('stepping pins the day window and Latest re-anchors', () => {
    const v = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={() => {}} />)
    fireEvent.click(v.getByRole('button', { name: 'Step back 1 day' }))
    // pinned one day back: start = 2023-12-29, end = 2024-01-04
    expect(caption(v.container)).toContain('2023-12-29')
    fireEvent.click(v.getByRole('button', { name: 'Reset to latest capture window' }))
    // back to data anchor: 2023-12-30 – 2024-01-05
    expect(caption(v.container)).toContain('2023-12-30')
  })

  it('clicking the Peak chip calls onBucketClick with day bounds', () => {
    const spy = vi.fn()
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={spy} />)
    fireEvent.click(getByRole('button', { name: /Peak/ }))
    expect(spy).toHaveBeenCalledTimes(1)
    // day bounds: since = T00:00:00Z, until = T23:59:59Z
    expect(spy.mock.calls[0][0]).toBe('2024-01-05T00:00:00Z')
    expect(spy.mock.calls[0][1]).toBe('2024-01-05T23:59:59Z')
  })

  it('keeps stepping enabled even when the target window has no data', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={() => {}} />)
    expect((getByRole('button', { name: 'Step back 1 day' }) as HTMLButtonElement).disabled).toBe(false)
    expect((getByRole('button', { name: 'Step forward 1 day' }) as HTMLButtonElement).disabled).toBe(false)
  })

  it('provides a Latest button with the 7-day title', () => {
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={() => {}} />)
    const btn = getByRole('button', { name: 'Reset to latest capture window' })
    expect(btn.getAttribute('title')).toMatch(/7 days/)
  })
})

// ── All (daily, full span) mode ───────────────────────────────────────────────

describe('TimeGraph all', () => {
  it('shows Total / Avg req/day chips with no window suffix', () => {
    const { container } = render(<TimeGraph hourly={makeHourly()} window="all" onBucketClick={() => {}} />)
    // "Total" without a window suffix (no "24h" or "7d")
    expect(container.textContent).toContain('Total')
    expect(container.textContent).not.toContain('Total 24h')
    expect(container.textContent).not.toContain('Total 7d')
    expect(container.textContent).toContain('Avg req/day')
  })

  it('spans first..latest day of the filtered set', () => {
    // Single-bucket fixture: first = latest = 2024-01-05 → span is 1 day
    const { container } = render(<TimeGraph hourly={makeHourly()} window="all" onBucketClick={() => {}} />)
    expect(caption(container)).toContain('2024-01-05')
  })

  it('spans the full multi-day range with gap days as null', () => {
    const { container } = render(<TimeGraph hourly={MULTI} window="all" onBucketClick={() => {}} />)
    // firstHour = 2024-01-01T10:00:00Z → day 2024-01-01
    // latestHour = 2024-01-03T09:00:00Z → day 2024-01-03
    // span = [2024-01-01, 2024-01-02, 2024-01-03]
    expect(caption(container)).toContain('2024-01-01')
    expect(caption(container)).toContain('2024-01-03')
    // Total = 8 + 2 = 10 (gap day 01-02 contributes 0)
    expect(container.textContent).toContain('Total')
    expect(container.textContent).toContain('10')
  })

  it('hides stepper and Latest buttons under All', () => {
    const { queryByRole } = render(<TimeGraph hourly={makeHourly()} window="all" onBucketClick={() => {}} />)
    expect(queryByRole('button', { name: 'Step back 1 day' })).toBeNull()
    expect(queryByRole('button', { name: 'Step forward 1 day' })).toBeNull()
    expect(queryByRole('button', { name: 'Reset to latest capture window' })).toBeNull()
  })

  it('clicking the Peak chip calls onBucketClick with day bounds', () => {
    const spy = vi.fn()
    const { getByRole } = render(<TimeGraph hourly={makeHourly()} window="all" onBucketClick={spy} />)
    fireEvent.click(getByRole('button', { name: /Peak/ }))
    expect(spy).toHaveBeenCalledTimes(1)
    expect(spy.mock.calls[0][0]).toBe('2024-01-05T00:00:00Z')
    expect(spy.mock.calls[0][1]).toBe('2024-01-05T23:59:59Z')
  })
})

// ── Chip-switch remount resets pins ───────────────────────────────────────────

describe('TimeGraph chip remount', () => {
  it('a fresh mount in day mode starts at the data anchor, not a stale hour pin', () => {
    // Simulate: user steps back in 24h mode (pins manual), then switches to 7d.
    // In production, key={statWin} remounts the component → manual resets.
    const v1 = render(<TimeGraph hourly={makeHourly()} window="24h" onBucketClick={() => {}} />)
    fireEvent.click(v1.getByRole('button', { name: 'Step back 1 hour' }))
    // pinned one hour back
    expect(caption(v1.container)).toContain('2024-01-04 20:00')
    v1.unmount()

    // Fresh mount in 7d mode: no pin, data-anchored at latest day − 6
    const v2 = render(<TimeGraph hourly={makeHourly()} window="7d" onBucketClick={() => {}} />)
    expect(caption(v2.container)).toContain('2023-12-30')
    expect(caption(v2.container)).toContain('2024-01-05')
  })
})
