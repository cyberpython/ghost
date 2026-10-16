import type { CaptureDetail } from '../api'
export function upstreamUrl(d: CaptureDetail){
  return d.upstream_scheme+'://'+d.upstream_authority+d.upstream_path+(d.upstream_query?'?'+d.upstream_query:'')
}
