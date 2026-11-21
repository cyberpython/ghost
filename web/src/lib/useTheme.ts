import { useEffect, useState } from 'react'

// Tracks the document's data-theme attribute so canvas-drawn elements
// (Chart.js) can re-resolve CSS variables when the theme toggles.
export function useTheme(): string {
  const [theme, setTheme] = useState<string>(() =>
    document.documentElement.getAttribute('data-theme') ?? '')
  useEffect(() => {
    const obs = new MutationObserver(() => {
      setTheme(document.documentElement.getAttribute('data-theme') ?? '')
    })
    obs.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] })
    return () => obs.disconnect()
  }, [])
  return theme
}

// Resolves a CSS custom property on :root to its concrete value, re-reading
// whenever the theme changes (canvas-drawn colors can't use var() directly).
export function useCssVar(name: string): string {
  // useTheme() re-renders on theme change, so the var is re-read each render.
  useTheme()
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim()
}
