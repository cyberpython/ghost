//! Path mapping between the client-facing target and the fixed upstream base.
//!
//! A client request target (e.g. `/endpoint/path?q=1`) is joined onto the
//! upstream base's path prefix (e.g. `/app`) to produce the upstream URL
//! (e.g. `http://host:port/app/endpoint/path?q=1`). Only the target path is
//! ever altered; everything else in the request is forwarded as-is.

use http::Uri;

/// Parse a configured `--upstream` URL into its components.
#[derive(Debug, Clone)]
pub struct UpstreamBase {
    /// Always `http` for this tool.
    pub scheme: String,
    /// `host` or `host:port` (with a default port appended when missing).
    pub authority: String,
    /// Path prefix the client path is appended to (e.g. `/app`), no trailing `/`.
    pub prefix: String,
}

impl UpstreamBase {
    /// Parse the upstream URL given on the command line.
    pub fn parse(s: &str) -> Result<Self, anyhow::Error> {
        let uri: Uri = s
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid --upstream {s:?}: {e}"))?;
        let scheme = match uri.scheme() {
            Some(scheme) if scheme.as_str() == "http" => "http".to_string(),
            Some(scheme) => {
                anyhow::bail!("only http:// upstreams are supported, got {scheme}://")
            }
            None => anyhow::bail!("--upstream must be an absolute URL, got {s:?}"),
        };
        let authority = match uri.authority() {
            Some(a) => a.to_string(),
            None => anyhow::bail!("--upstream must include a host, got {s:?}"),
        };
        // A default-port HTTP authority without an explicit port needs one for
        // the TCP connect; `Uri` keeps it off when it equals the scheme default.
        let authority = if authority.contains(':') {
            authority
        } else {
            format!("{authority}:80")
        };
        let prefix = uri.path().to_string();
        Ok(Self { scheme, authority, prefix })
    }

    /// Resolve a client request target into the upstream URI plus the parts we
    /// store (upstream path and query).
    ///
    /// `target` is the client request target with origin removed, i.e.
    /// `path` or `path?query` (both may be empty).
    pub fn resolve(&self, target: &str) -> (Uri, String, Option<String>) {
        let (path, query) = split_target(target);
        let joined = join_prefix(&self.prefix, path);
        let mut url = format!("{}://{}{}", self.scheme, self.authority, joined);
        if let Some(q) = &query {
            url.push('?');
            url.push_str(q);
        }
        // The client target is well-formed for this proxy, so this cannot fail.
        let uri = url
            .parse::<Uri>()
            .unwrap_or_else(|_| format!("{}://{}{}", self.scheme, self.authority, "/").parse::<Uri>().unwrap());
        let query = query.map(|s| s.to_string());
        (uri, joined, query)
    }
}

/// Split a request target into its path and query (`""` when no query).
pub fn split_target(target: &str) -> (&str, Option<&str>) {
    match target.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (target, None),
    }
}

/// Join the upstream path prefix onto a client path.
///
/// - `prefix` is the upstream base path (e.g. `/app` or `""`).
/// - `client_path` is the client request path (`"/"`, `"/endpoint/path"`, or `""`).
pub fn join_prefix(prefix: &str, client_path: &str) -> String {
    // Normalise the prefix: strip trailing slashes ("/app/" -> "/app", "/" -> "").
    let prefix = prefix.trim_end_matches('/');
    if client_path.is_empty() {
        return prefix.to_string();
    }
    if client_path.starts_with('/') {
        format!("{prefix}{client_path}")
    } else {
        // Defensive: an HTTP request path is always absolute; keep it sound.
        format!("{prefix}/{client_path}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_basics() {
        assert_eq!(join_prefix("/app", "/endpoint/path"), "/app/endpoint/path");
        assert_eq!(join_prefix("/app", "/"), "/app/");
        assert_eq!(join_prefix("", "/endpoint/path"), "/endpoint/path");
        assert_eq!(join_prefix("", "/"), "/");
        assert_eq!(join_prefix("/app", ""), "/app");
        assert_eq!(join_prefix("", ""), "");
        assert_eq!(join_prefix("/app/", "/endpoint"), "/app/endpoint");
        assert_eq!(join_prefix("/", "/endpoint"), "/endpoint");
    }

    #[test]
    fn split_target_basics() {
        assert_eq!(split_target("/endpoint/path"), ("/endpoint/path", None));
        assert_eq!(split_target("/endpoint/path?q=1"), ("/endpoint/path", Some("q=1")));
        assert_eq!(split_target("/?q=1"), ("/", Some("q=1")));
        assert_eq!(split_target(""), ("", None));
    }

    #[test]
    fn resolve_full() {
        let up = UpstreamBase::parse("http://127.0.0.1:9000/app").unwrap();
        let (uri, path, query) = up.resolve("/endpoint/path?q=1");
        assert_eq!(uri.to_string(), "http://127.0.0.1:9000/app/endpoint/path?q=1");
        assert_eq!(path, "/app/endpoint/path");
        assert_eq!(query, Some("q=1".to_string()));

        let (uri, path, query) = up.resolve("/");
        assert_eq!(uri.to_string(), "http://127.0.0.1:9000/app/");
        assert_eq!(path, "/app/");
        assert_eq!(query, None);

        let (uri, path, query) = up.resolve("");
        assert_eq!(uri.to_string(), "http://127.0.0.1:9000/app");
        assert_eq!(path, "/app");
        assert_eq!(query, None);
    }

    #[test]
    fn default_port_is_added() {
        let up = UpstreamBase::parse("http://example.org/app").unwrap();
        assert_eq!(up.authority, "example.org:80");
        let (uri, _, _) = up.resolve("/x");
        assert_eq!(uri.to_string(), "http://example.org:80/app/x");
    }

    #[test]
    fn reject_non_http() {
        assert!(UpstreamBase::parse("https://example.org/app").is_err());
        assert!(UpstreamBase::parse("/app").is_err());
    }
}
