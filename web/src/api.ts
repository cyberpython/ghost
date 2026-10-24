export type Hdr = { name: string; value: string }
export type BodyView = { text: string | null; text_truncated: boolean; binary: boolean;
  hex_preview: string | null; len: number; sha256: string | null; truncated: boolean }
export type CaptureListItem = {
  id: number; created_at: string; total_ms: number; client_addr: string;
  request_method: string; request_target: string; upstream_path: string; upstream_query: string | null;
  status_code: number | null; error: string | null;
  request_body_len: number; request_body_truncated: boolean;
  response_body_len: number; response_body_truncated: boolean }
type RawCaptureListItem = Omit<CaptureListItem,
  'request_body_truncated' | 'response_body_truncated'>
  & { request_body_truncated: number; response_body_truncated: number }
export type CaptureDetail = {
  id: number; created_at: string; total_ms: number; client_addr: string;
  request_version: string; request_method: string; request_target: string;
  request_headers: Hdr[]; request_body: BodyView;
  upstream_scheme: string; upstream_authority: string; upstream_path: string; upstream_query: string | null;
  response_version: string | null; status_code: number | null; response_headers: Hdr[];
  response_body: BodyView; error: string | null }
export type Facets = { methods: string[]; statuses: number[]; errors: number }

export async function listCaptures(q: { method?: string; status?: number | null; path?: string; client_addr?: string;
  since?: string; until?: string; limit?: number; offset?: number }) {
  const p = new URLSearchParams()
  for (const [k, v] of Object.entries(q)) if (v !== undefined) p.set(k, String(v))
  const r = await fetch(`/api/captures?${p.toString()}`)
  const j = (await r.json()) as { count: number; captures: RawCaptureListItem[] }
  return { count: j.count, captures: j.captures.map(c => ({
    ...c,
    request_body_truncated: c.request_body_truncated !== 0,
    response_body_truncated: c.response_body_truncated !== 0,
  })) }
}
export async function getCapture(id: number) {
  const r = await fetch(`/api/captures/${id}`)
  if (!r.ok) throw new Error('http ' + r.status)
  return (await r.json()) as CaptureDetail
}
export async function getFacets() {
  return (await (await fetch('/api/facets')).json()) as Facets
}
