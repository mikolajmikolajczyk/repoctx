//! HTTP fetcher with on-disk cache for per-agent integration files.
//!
//! Files are pulled from the GitHub mirror at a pinned ref (default
//! `v<CARGO_PKG_VERSION>`), then cached under the user's XDG cache dir
//! so re-installs and offline use stay cheap.

use std::fs;
use std::path::PathBuf;

use directories::ProjectDirs;
use tracing::debug;

use crate::error::{IntegrationsError, Result};
use crate::manifest::Agent;

const DEFAULT_BASE_URL: &str = "https://raw.githubusercontent.com/mikolajmikolajczyk/repoctx";
const MANIFEST_FILE: &str = "manifest.toml";

/// Pluggable HTTP layer so tests can inject an in-memory source.
pub trait HttpFetch: Send + Sync {
    fn get(&self, url: &str) -> std::result::Result<Vec<u8>, String>;
}

/// Default HTTP fetcher backed by `ureq` + rustls.
pub struct UreqFetch {
    agent: ureq::Agent,
}

impl UreqFetch {
    pub fn new() -> Self {
        Self {
            agent: ureq::AgentBuilder::new()
                .timeout(std::time::Duration::from_secs(30))
                .build(),
        }
    }
}

impl Default for UreqFetch {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetch for UreqFetch {
    fn get(&self, url: &str) -> std::result::Result<Vec<u8>, String> {
        use std::io::Read;
        let resp = self.agent.get(url).call().map_err(|e| format!("{e}"))?;
        let mut buf = Vec::new();
        resp.into_reader()
            .read_to_end(&mut buf)
            .map_err(|e| format!("read body: {e}"))?;
        Ok(buf)
    }
}

/// Fetcher composes a remote source + an on-disk cache. Reads come from
/// cache when present (unless `no_cache`); writes always populate cache
/// so the next call is free.
pub struct Fetcher {
    base_url: String,
    cache_dir: PathBuf,
    ref_: String,
    no_cache: bool,
    http: Box<dyn HttpFetch>,
}

impl Fetcher {
    /// Build with defaults: GitHub raw base, XDG cache dir, ureq fetch.
    /// `ref_` defaults to `v<CARGO_PKG_VERSION>` when `None`.
    pub fn new(ref_: Option<String>, no_cache: bool) -> Result<Self> {
        let cache_dir = default_cache_dir()?;
        Ok(Self {
            base_url: DEFAULT_BASE_URL.to_string(),
            cache_dir,
            ref_: ref_.unwrap_or_else(default_ref),
            no_cache,
            http: Box::new(UreqFetch::new()),
        })
    }

    /// Test/custom constructor: caller wires base URL, cache dir, and
    /// HTTP backend by hand.
    pub fn with_parts(
        base_url: impl Into<String>,
        cache_dir: impl Into<PathBuf>,
        ref_: impl Into<String>,
        no_cache: bool,
        http: Box<dyn HttpFetch>,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            cache_dir: cache_dir.into(),
            ref_: ref_.into(),
            no_cache,
            http,
        }
    }

    /// The git ref this fetcher pins. Useful for status reporting.
    pub fn ref_(&self) -> &str {
        &self.ref_
    }

    /// Fetch and parse an agent's manifest.
    pub fn fetch_manifest(&self, agent: &str) -> Result<Agent> {
        validate_agent_name(agent)?;
        let bytes = self.fetch_bytes(agent, MANIFEST_FILE)?;
        let text = std::str::from_utf8(&bytes).map_err(|e| IntegrationsError::ManifestInvalid {
            path: PathBuf::from(format!("integrations/{agent}/{MANIFEST_FILE}")),
            reason: format!("manifest is not valid UTF-8: {e}"),
        })?;
        Agent::from_toml(
            text,
            &PathBuf::from(format!("integrations/{agent}/{MANIFEST_FILE}")),
        )
    }

    /// Fetch one file referenced by a manifest. `src` is the manifest's
    /// `file.src` value — may use `../shared/...` to reach the shared dir.
    pub fn fetch_file(&self, agent: &str, src: &str) -> Result<Vec<u8>> {
        validate_agent_name(agent)?;
        self.fetch_bytes(agent, src)
    }

    fn fetch_bytes(&self, agent: &str, rel: &str) -> Result<Vec<u8>> {
        let (cache_path, normalized_url_path) = self.resolve_paths(agent, rel)?;
        if !self.no_cache {
            if let Ok(buf) = fs::read(&cache_path) {
                debug!(?cache_path, "fetcher: cache hit");
                return Ok(buf);
            }
        }
        let url = format!(
            "{base}/{ref_}/integrations/{path}",
            base = self.base_url,
            ref_ = self.ref_,
            path = normalized_url_path,
        );
        debug!(%url, "fetcher: GET");
        let bytes = self.http.get(&url).map_err(|e| IntegrationsError::Fetch {
            url: url.clone(),
            cache_path: cache_path.clone(),
            message: e,
        })?;
        if let Some(parent) = cache_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        if let Err(e) = fs::write(&cache_path, &bytes) {
            debug!(?cache_path, error = %e, "fetcher: cache write failed (non-fatal)");
        }
        Ok(bytes)
    }

    fn resolve_paths(&self, agent: &str, rel: &str) -> Result<(PathBuf, String)> {
        // Compose `<agent>/<rel>`, then normalize ../ semantically. The
        // result must stay inside `integrations/`.
        let mut comps: Vec<String> = vec![agent.to_string()];
        for seg in rel.split('/').filter(|s| !s.is_empty()) {
            match seg {
                "." => {}
                ".." => {
                    if comps.pop().is_none() {
                        return Err(IntegrationsError::ManifestInvalid {
                            path: PathBuf::from(rel),
                            reason: format!("src `{rel}` escapes the integrations/ root"),
                        });
                    }
                }
                other => comps.push(other.to_string()),
            }
        }
        if comps.is_empty() {
            return Err(IntegrationsError::ManifestInvalid {
                path: PathBuf::from(rel),
                reason: format!("src `{rel}` resolves to empty path"),
            });
        }
        let url_path = comps.join("/");
        let mut cache = self.cache_dir.join(&self.ref_);
        for c in &comps {
            cache.push(c);
        }
        Ok((cache, url_path))
    }
}

fn validate_agent_name(agent: &str) -> Result<()> {
    if !crate::AGENTS.contains(&agent) {
        return Err(IntegrationsError::UnknownAgent(agent.to_string()));
    }
    Ok(())
}

fn default_ref() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

fn default_cache_dir() -> Result<PathBuf> {
    let dirs =
        ProjectDirs::from("dev", "repoctx", "repoctx").ok_or_else(|| IntegrationsError::Cache {
            path: PathBuf::new(),
            reason: "no project-dirs path available on this platform".into(),
        })?;
    Ok(dirs.cache_dir().join("integrations"))
}

/// Public so the CLI can show "where would the cache live" in error
/// messages without instantiating a Fetcher.
pub fn cache_dir_for_diagnostics() -> Option<PathBuf> {
    default_cache_dir().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    struct InMemoryFetch {
        files: HashMap<String, Vec<u8>>,
        calls: Mutex<Vec<String>>,
    }

    impl InMemoryFetch {
        fn with(files: &[(&str, &str)]) -> Self {
            Self {
                files: files
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.as_bytes().to_vec()))
                    .collect(),
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl HttpFetch for InMemoryFetch {
        fn get(&self, url: &str) -> std::result::Result<Vec<u8>, String> {
            self.calls.lock().unwrap().push(url.to_string());
            self.files
                .get(url)
                .cloned()
                .ok_or_else(|| format!("404 (test): {url}"))
        }
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn manifest_url(base: &str, ref_: &str, agent: &str) -> String {
        format!("{base}/{ref_}/integrations/{agent}/manifest.toml")
    }

    const VALID_TOML: &str = r#"
name = "claude"
description = "test"
[[file]]
src = "SKILL.md"
dest = ".claude/skills/repoctx/SKILL.md"
mode = "write"
"#;

    #[test]
    fn fetches_manifest_and_caches() {
        let cache = tmp();
        let http =
            InMemoryFetch::with(&[(&manifest_url("https://x", "main", "claude"), VALID_TOML)]);
        let f = Fetcher::with_parts("https://x", cache.path(), "main", false, Box::new(http));
        let a = f.fetch_manifest("claude").unwrap();
        assert_eq!(a.name, "claude");

        // Second call hits cache; we can prove that by replacing the http
        // layer with one that would 404.
        let http2 = InMemoryFetch::default();
        let f2 = Fetcher::with_parts("https://x", cache.path(), "main", false, Box::new(http2));
        let a2 = f2.fetch_manifest("claude").unwrap();
        assert_eq!(a2.name, "claude");
    }

    #[test]
    fn no_cache_skips_disk_read() {
        let cache = tmp();
        // Pre-populate cache with stale content.
        let agent_dir = cache.path().join("main/claude");
        fs::create_dir_all(&agent_dir).unwrap();
        fs::write(agent_dir.join("manifest.toml"), b"stale").unwrap();

        let http =
            InMemoryFetch::with(&[(&manifest_url("https://x", "main", "claude"), VALID_TOML)]);
        let f = Fetcher::with_parts("https://x", cache.path(), "main", true, Box::new(http));
        let a = f.fetch_manifest("claude").unwrap();
        assert_eq!(a.name, "claude");
    }

    #[test]
    fn offline_with_no_cache_errors_with_url_and_cache_path() {
        let cache = tmp();
        let http = InMemoryFetch::default();
        let f = Fetcher::with_parts("https://x", cache.path(), "main", false, Box::new(http));
        let err = f.fetch_manifest("claude").unwrap_err();
        match err {
            IntegrationsError::Fetch {
                url, cache_path, ..
            } => {
                assert!(url.contains("/main/integrations/claude/manifest.toml"));
                assert!(cache_path.ends_with("main/claude/manifest.toml"));
            }
            other => panic!("expected Fetch error, got {other:?}"),
        }
    }

    #[test]
    fn unknown_agent_rejected() {
        let cache = tmp();
        let f = Fetcher::with_parts(
            "https://x",
            cache.path(),
            "main",
            false,
            Box::new(InMemoryFetch::default()),
        );
        let err = f.fetch_manifest("aider").unwrap_err();
        assert!(matches!(err, IntegrationsError::UnknownAgent(_)));
    }

    #[test]
    fn resolves_shared_via_parent_dir() {
        let cache = tmp();
        let url = "https://x/main/integrations/shared/AGENTS.md.fragment".to_string();
        let http = InMemoryFetch::with(&[(&url, "hello")]);
        let f = Fetcher::with_parts("https://x", cache.path(), "main", false, Box::new(http));
        let bytes = f
            .fetch_file("codex", "../shared/AGENTS.md.fragment")
            .unwrap();
        assert_eq!(bytes, b"hello");
        // Cache lands under shared/, not codex/, after `..` normalization.
        assert!(cache.path().join("main/shared/AGENTS.md.fragment").exists());
    }

    #[test]
    fn rejects_src_escaping_integrations_root() {
        let cache = tmp();
        let f = Fetcher::with_parts(
            "https://x",
            cache.path(),
            "main",
            false,
            Box::new(InMemoryFetch::default()),
        );
        let err = f.fetch_file("claude", "../../etc/passwd").unwrap_err();
        assert!(matches!(err, IntegrationsError::ManifestInvalid { .. }));
    }
}
