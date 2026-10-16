import { statusColor } from '../lib/colors'
export default function StatusBadge({ code }: { code: number | null }) {
  return <span className="inline-block px-2 py-0.5 rounded-full text-sm font-semibold text-[#0b0e14]" style={{ backgroundColor: statusColor(code) }}>{code === null ? 'ERR' : code}</span>
}
