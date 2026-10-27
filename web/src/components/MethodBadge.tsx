import { methodColor } from '../lib/colors'
export default function MethodBadge({ m }: { m: string }) {
  return <span className="badge method inline-block px-2 py-0.5 text-sm font-semibold text-[#0b0e14]" style={{ backgroundColor: methodColor(m) }}>{m}</span>
}
