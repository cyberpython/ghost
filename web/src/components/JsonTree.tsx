import { useState } from 'react'

// FR-026b: dependency-free collapsible JSON tree for the BodyViewer Tree tab.
// It renders on the same fixed-dark block as the Prism views (`.json-tree` in
// index.css); the token colors below are lifted from the Prism `tomorrow`
// style (lib/highlight.tsx) so the Tree tab matches Formatted/Raw in both
// themes.

export type Json = null | boolean | number | string | Json[] | { [key: string]: Json }

type Container = Json[] | { [key: string]: Json }

// Display cap for string leaves; the Raw tab still carries the full value.
const LEAF_CAP = 300
// Arrays above this size start collapsed even at shallow depth.
const LONG_ARRAY = 20

const C = {
  key: '#f8c555', // prism property
  string: '#7ec699', // prism string
  number: '#f08d49', // prism number / boolean
  null: '#cc99cd', // prism null (keyword alias)
  punct: '#ccc', // prism punctuation
  operator: '#67cdcc', // prism operator (":")
  muted: '#999', // prism comment — arrows and summaries
}

function isObject(v: Json): v is { [key: string]: Json } {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}

function KeyLabel({ name }: { name: string }) {
  return (
    <>
      <span style={{ color: C.key }}>{JSON.stringify(name)}</span>
      <span style={{ color: C.operator }}>{': '}</span>
    </>
  )
}

function Leaf({ name, value }: { name?: string; value: string | number | boolean | null }) {
  let text: string
  let color: string
  let title: string | undefined
  if (typeof value === 'string') {
    if (value.length > LEAF_CAP) {
      text = `${JSON.stringify(value.slice(0, LEAF_CAP))}…`
      title = `value truncated for display (${value.length} chars) — see the Raw tab for the full value`
    } else {
      text = JSON.stringify(value)
    }
    color = C.string
  } else if (typeof value === 'number') {
    text = JSON.stringify(value)
    color = C.number
  } else if (typeof value === 'boolean') {
    text = String(value)
    color = C.number
  } else {
    text = 'null'
    color = C.null
  }
  return (
    <div title={title}>
      {name !== undefined && <KeyLabel name={name} />}
      <span style={{ color }}>{text}</span>
    </div>
  )
}

function Node({ name, value, depth, allOpen }: { name?: string; value: Container; depth: number; allOpen: boolean | null }) {
  const object = isObject(value)
  const entries: [string | null, Json][] = object
    ? Object.keys(value).map(k => [k, value[k]] as [string, Json])
    : value.map(v => [null, v] as [string | null, Json])
  const count = entries.length
  // Containers shallower than depth 2 open by default; long arrays stay shut
  // wherever they sit. `allOpen` (Expand all / Collapse all) overrides the
  // rule entirely. Collapsed children are not mounted at all, so the
  // rendered DOM stays proportional to what has been opened.
  const [open, setOpen] = useState(allOpen ?? (depth < 2 && (object || count <= LONG_ARRAY)))
  const close = object ? '}' : ']'

  if (count === 0) {
    return (
      <div>
        {name !== undefined && <KeyLabel name={name} />}
        <span style={{ color: C.punct }}>{object ? '{}' : '[]'}</span>
      </div>
    )
  }

  return (
    <div>
      <button type="button" aria-expanded={open} onClick={() => setOpen(o => !o)}
        className="block w-full text-left cursor-pointer hover:bg-[#333]">
        <span style={{ color: C.muted }}>{open ? '▾ ' : '▸ '}</span>
        {name !== undefined && <KeyLabel name={name} />}
        <span style={{ color: C.punct }}>{object ? '{' : '['}</span>
        {!open && (
          <>
            <span style={{ color: C.punct }}>{` … ${close}`}</span>
            <span style={{ color: C.muted }}>
              {` ${count} ${count === 1 ? (object ? 'key' : 'item') : object ? 'keys' : 'items'}`}
            </span>
          </>
        )}
      </button>
      {open && (
        <div className="ml-4 pl-3 border-l border-[#3d3d3d]">
          {entries.map(([k, v], i) =>
            isObject(v) || Array.isArray(v)
              ? <Node key={k ?? i} name={k ?? undefined} value={v} depth={depth + 1} allOpen={allOpen} />
              : <Leaf key={k ?? i} name={k ?? undefined} value={v} />)}
          <div style={{ color: C.punct }}>{close}</div>
        </div>
      )}
    </div>
  )
}

export default function JsonTree({ value }: { value: Json }) {
  // Expand all / Collapse all: one bump per press; the key change remounts
  // the tree so every Node re-initializes from `allOpen`. Manual toggles are
  // forgotten — that is what a global control means. The buttons sit pinned
  // at the top right of the block, outside the scrollable area.
  const [epoch, setEpoch] = useState(0)
  const [allOpen, setAllOpen] = useState<boolean | null>(null)
  const applyAll = (to: boolean) => { setAllOpen(to); setEpoch(e => e + 1) }
  const ctlCls = 'text-xs px-2 py-0.5 rounded bg-[#3a3a3a] text-[#ccc] border border-[#555] hover:bg-[#444] cursor-pointer'
  return (
    <div className="relative">
      <div className="absolute top-1 right-1 z-10 flex gap-1">
        <button type="button" className={ctlCls} onClick={() => applyAll(false)}>Collapse all</button>
        <button type="button" className={ctlCls} onClick={() => applyAll(true)}>Expand all</button>
      </div>
      <div className="json-tree" key={epoch}>
        {isObject(value) || Array.isArray(value)
          ? <Node value={value} depth={0} allOpen={allOpen} />
          : <Leaf value={value} />}
      </div>
    </div>
  )
}
