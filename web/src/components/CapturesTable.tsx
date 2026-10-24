import { Link, useNavigate } from 'react-router'
import type { CaptureListItem } from '../api'
import { formatSize } from '../lib/formatSize'
import MethodBadge from './MethodBadge'
import StatusBadge from './StatusBadge'
export default function CapturesTable({ captures }: { captures: CaptureListItem[] }) {
  const navigate = useNavigate()
  return (
    <table className="border-collapse w-full">
      <thead>
        <tr className="border-b border-[var(--border)]">
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">Time</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">Method</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">Target</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">Status</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">ms</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium" title="Request body size (stored)">Body</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">Client</th>
          <th className="text-left px-2.5 py-1.5 text-sm font-medium">Error</th>
        </tr>
      </thead>
      <tbody>
        {captures.map(row => (
          <tr key={row.id} className={`cursor-pointer border-b border-[var(--border)] hover:bg-[var(--panel)] ${row.error ? 'bg-[color-mix(in_srgb,var(--s-5xx)_18%,transparent)]' : ''}`}
            onClick={e => {
              if (e.target instanceof HTMLAnchorElement) return
              navigate(`/captures/${row.id}`)
            }}>
            <td className="px-2.5 py-1.5 whitespace-nowrap tabular-nums"><Link to={`/captures/${row.id}`}>{row.created_at}</Link></td>
            <td className="px-2.5 py-1.5"><MethodBadge m={row.request_method}/></td>
            <td className="px-2.5 py-1.5 break-all min-w-0">{row.request_target}</td>
            <td className="px-2.5 py-1.5"><StatusBadge code={row.status_code}/></td>
            <td className="px-2.5 py-1.5">{row.total_ms}</td>
            <td className="px-2.5 py-1.5 tabular-nums whitespace-nowrap" title={row.request_body_truncated ? 'body truncated (stored cap)' : undefined}>{formatSize(row.request_body_len)}{row.request_body_truncated ? ' *' : ''}</td>
            <td className="px-2.5 py-1.5">{row.client_addr}</td>
            <td className="px-2.5 py-1.5">{row.error ?? ''}</td>
          </tr>
        ))}
      </tbody>
    </table>
  )
}
