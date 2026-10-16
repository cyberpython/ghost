import { useState, useRef, useEffect } from 'react'

interface Props {
  value: string
  options: { label: string; value: string }[]
  onChange: (v: string) => void
  placeholder?: string
}

export default function SelectDropdown({ value, options, onChange, placeholder }: Props) {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false)
    }
    document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [])

  const selected = options.find(o => o.value === value)

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen(!open)}
        className="border border-[var(--border)] rounded px-2.5 py-1.5 text-sm bg-transparent text-[var(--text)] flex items-center justify-between gap-2 min-w-[120px] hover:border-[var(--accent)] transition-colors"
      >
        <span>{selected?.label || placeholder}</span>
        <svg className="w-3 h-3 shrink-0" fill="none" viewBox="0 0 12 12" stroke="currentColor" strokeOpacity={0.6}>
          <path d="M3 5l3 3 3-3" strokeWidth={1.5} strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
      </button>
      {open && (
        <div className="absolute top-full left-0 mt-1 bg-[var(--panel)] border border-[var(--border)] rounded shadow-lg z-50 min-w-full overflow-hidden"
          style={{ minWidth: 120 }}>
          {options.map(o => (
            <button
              key={o.value}
              onClick={() => { onChange(o.value); setOpen(false) }}
              className={`w-full text-left px-2.5 py-1.5 text-sm hover:bg-[var(--accent)] transition-colors ${
                o.value === value ? 'bg-[var(--accent)] text-[var(--bg)] font-semibold' : 'text-[var(--text)]'
              }`}
            >
              {o.label}
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
