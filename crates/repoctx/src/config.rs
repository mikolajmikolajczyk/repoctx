//! Per-repo config layer.
//!
//! Backed by the `settings` table inside `.repoctx/index.db` (schema v3).
//! Precedence (highest wins):
//!
//! 1. CLI flag on this invocation (applied by the command handler).
//! 2. Environment variable (`REPOCTX_<SECTION>_<KEY>`).
//! 3. Stored `settings` row.
//! 4. Built-in default.
//!
//! See `wiki/decisions/2026-06-12-config-schema.md` for the binding
//! contract.

use std::env;

use anyhow::{anyhow, Result};
use repoctx_store::Store;
use tracing::warn;

/// Default `analysis.subsystem_min_size` (see [`AnalysisConfig`]).
pub const DEFAULT_SUBSYSTEM_MIN_SIZE: usize = 5;

/// Where a config value came from. Used by `config show` to annotate
/// each row with its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Env,
    Settings,
    Default,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Env => "env",
            Self::Settings => "settings",
            Self::Default => "default",
        }
    }
}

/// `output.default` — persistent output-format choice. `Auto` matches
/// the existing TTY/non-TTY detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputDefault {
    Auto,
    Human,
    Toon,
    Json,
}

impl OutputDefault {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Human => "human",
            Self::Toon => "toon",
            Self::Json => "json",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "human" => Ok(Self::Human),
            "toon" => Ok(Self::Toon),
            "json" => Ok(Self::Json),
            other => Err(anyhow!(
                "expected one of [auto, human, toon, json] (got '{other}')"
            )),
        }
    }
}

fn parse_bool(s: &str) -> Result<bool> {
    match s.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Ok(true),
        "false" | "0" | "no" => Ok(false),
        other => Err(anyhow!("expected true|false|1|0|yes|no (got '{other}')")),
    }
}

/// Parse + validate `analysis.subsystem_min_size`: a positive integer (a
/// subsystem needs at least 2 members to be a cluster at all).
fn parse_subsystem_min_size(s: &str) -> Result<usize> {
    let n: usize = s
        .trim()
        .parse()
        .map_err(|_| anyhow!("expected a positive integer (got '{s}')"))?;
    if n < 2 {
        return Err(anyhow!("must be >= 2 (a cluster needs >= 2 members)"));
    }
    Ok(n)
}

fn fmt_bool(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

/// Resolved per-section config. Each field carries its value AND the
/// `Source` that supplied it.
#[derive(Debug, Clone)]
pub struct GainConfig {
    pub no_record: bool,
    pub no_record_source: Source,
    pub record_query: bool,
    pub record_query_source: Source,
}

#[derive(Debug, Clone)]
pub struct OutputConfig {
    pub default: OutputDefault,
    pub default_source: Source,
}

#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Capture nested keys in JSON/YAML/TOML (opt-in). Re-index required
    /// after flipping (`repoctx index --force`).
    pub nested_keys: bool,
    pub nested_keys_source: Source,
}

#[derive(Debug, Clone)]
pub struct AnalysisConfig {
    /// Minimum member count for a Louvain cluster to count as a "subsystem"
    /// (`communities`/`report`/`export` share this definition so their counts
    /// agree). Pairs/tiny tails below this are not subsystems. Default 5.
    pub subsystem_min_size: usize,
    pub subsystem_min_size_source: Source,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub gain: GainConfig,
    pub output: OutputConfig,
    pub index: IndexConfig,
    pub analysis: AnalysisConfig,
}

impl Config {
    /// Built-in defaults. Used as a starting point if there's no DB to
    /// read from (e.g. tests).
    pub fn defaults() -> Self {
        Self {
            gain: GainConfig {
                no_record: false,
                no_record_source: Source::Default,
                record_query: false,
                record_query_source: Source::Default,
            },
            output: OutputConfig {
                default: OutputDefault::Auto,
                default_source: Source::Default,
            },
            index: IndexConfig {
                nested_keys: false,
                nested_keys_source: Source::Default,
            },
            analysis: AnalysisConfig {
                subsystem_min_size: DEFAULT_SUBSYSTEM_MIN_SIZE,
                subsystem_min_size_source: Source::Default,
            },
        }
    }

    /// Layer settings + env on top of defaults.
    pub fn load(store: &Store) -> Result<Self> {
        let mut cfg = Self::defaults();
        Self::apply_settings(&mut cfg, store)?;
        Self::apply_env(&mut cfg);
        Ok(cfg)
    }

    fn apply_settings(cfg: &mut Self, store: &Store) -> Result<()> {
        for (key, value) in store.all_settings()? {
            match key.as_str() {
                // The per-command rewrite hook was removed (adoption pivoted to
                // session-start priming, 2026-06-16). Old `hook.*` rows from
                // prior installs are ignored quietly, not warned about.
                k if k.starts_with("hook.") => {}
                "gain.no_record" => match parse_bool(&value) {
                    Ok(v) => {
                        cfg.gain.no_record = v;
                        cfg.gain.no_record_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                "gain.record_query" => match parse_bool(&value) {
                    Ok(v) => {
                        cfg.gain.record_query = v;
                        cfg.gain.record_query_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                "index.nested_keys" => match parse_bool(&value) {
                    Ok(v) => {
                        cfg.index.nested_keys = v;
                        cfg.index.nested_keys_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                "output.default" => match OutputDefault::parse(&value) {
                    Ok(v) => {
                        cfg.output.default = v;
                        cfg.output.default_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                "analysis.subsystem_min_size" => match parse_subsystem_min_size(&value) {
                    Ok(v) => {
                        cfg.analysis.subsystem_min_size = v;
                        cfg.analysis.subsystem_min_size_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                _ => {
                    warn!(key = %key, value = %value, "unknown config key in settings table");
                }
            }
        }
        Ok(())
    }

    fn apply_env(cfg: &mut Self) {
        if let Ok(v) = env::var("REPOCTX_GAIN_NO_RECORD") {
            match parse_bool(&v) {
                Ok(b) => {
                    cfg.gain.no_record = b;
                    cfg.gain.no_record_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_GAIN_NO_RECORD", &v, e),
            }
        }
        // Back-compat: existing RUST_REPOCTX_NO_RECORD continues to work.
        // Deprecated in favor of REPOCTX_GAIN_NO_RECORD; consumer logs a
        // warn.
        if cfg.gain.no_record_source == Source::Default
            && env::var_os("RUST_REPOCTX_NO_RECORD").is_some()
        {
            cfg.gain.no_record = true;
            cfg.gain.no_record_source = Source::Env;
        }
        if let Ok(v) = env::var("REPOCTX_GAIN_RECORD_QUERY") {
            match parse_bool(&v) {
                Ok(b) => {
                    cfg.gain.record_query = b;
                    cfg.gain.record_query_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_GAIN_RECORD_QUERY", &v, e),
            }
        }
        if let Ok(v) = env::var("REPOCTX_INDEX_NESTED_KEYS") {
            match parse_bool(&v) {
                Ok(b) => {
                    cfg.index.nested_keys = b;
                    cfg.index.nested_keys_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_INDEX_NESTED_KEYS", &v, e),
            }
        }
        if let Ok(v) = env::var("REPOCTX_OUTPUT_DEFAULT") {
            match OutputDefault::parse(&v) {
                Ok(o) => {
                    cfg.output.default = o;
                    cfg.output.default_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_OUTPUT_DEFAULT", &v, e),
            }
        }
        if let Ok(v) = env::var("REPOCTX_ANALYSIS_SUBSYSTEM_MIN_SIZE") {
            match parse_subsystem_min_size(&v) {
                Ok(n) => {
                    cfg.analysis.subsystem_min_size = n;
                    cfg.analysis.subsystem_min_size_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_ANALYSIS_SUBSYSTEM_MIN_SIZE", &v, e),
            }
        }
    }
}

fn warn_invalid<E: std::fmt::Display>(key: &str, value: &str, err: E) {
    warn!(
        key = %key,
        value = %value,
        error = %err,
        "ignoring invalid config value; falling back to default",
    );
}

/// Validate + write a setting through the store.
pub fn set(store: &mut Store, key: &str, value: &str) -> Result<String> {
    let normalized = match key {
        "gain.no_record" | "gain.record_query" | "index.nested_keys" => {
            fmt_bool(parse_bool(value)?).to_string()
        }
        "output.default" => OutputDefault::parse(value)?.as_str().to_string(),
        "analysis.subsystem_min_size" => parse_subsystem_min_size(value)?.to_string(),
        k if k.starts_with("hook.") => {
            return Err(anyhow!(
                "`{k}` is obsolete: the per-command rewrite hook was removed; \
                 repoctx now primes via a SessionStart hook (`repoctx init`)"
            ))
        }
        other => return Err(anyhow!("unknown config key: {other}")),
    };
    store.set_setting(key, &normalized)?;
    Ok(normalized)
}

/// All known config keys with their built-in defaults. Used by
/// `config show` to display every row even when none are stored.
pub fn known_keys() -> Vec<(&'static str, String)> {
    vec![
        ("gain.no_record", fmt_bool(false).to_string()),
        ("gain.record_query", fmt_bool(false).to_string()),
        ("output.default", OutputDefault::Auto.as_str().to_string()),
        ("index.nested_keys", fmt_bool(false).to_string()),
        (
            "analysis.subsystem_min_size",
            DEFAULT_SUBSYSTEM_MIN_SIZE.to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_default_parses_case_insensitive() {
        assert_eq!(OutputDefault::parse("Auto").unwrap(), OutputDefault::Auto);
        assert_eq!(OutputDefault::parse("JSON").unwrap(), OutputDefault::Json);
        assert!(OutputDefault::parse("xml").is_err());
    }

    #[test]
    fn obsolete_hook_keys_rejected_on_set_and_ignored_on_load() {
        let mut store = Store::open_in_memory().unwrap();
        // `config set hook.*` is rejected with an obsolete message.
        let err = set(&mut store, "hook.use_rtk", "on").unwrap_err();
        assert!(err.to_string().contains("obsolete"));
        // A stale hook.* row from an old install loads without error/warn noise.
        store.set_setting("hook.rewrite", "off").unwrap();
        assert!(Config::load(&store).is_ok());
    }

    #[test]
    fn bool_parses_truthy_falsy_forms() {
        assert!(parse_bool("true").unwrap());
        assert!(parse_bool("YES").unwrap());
        assert!(parse_bool("1").unwrap());
        assert!(!parse_bool("false").unwrap());
        assert!(!parse_bool("no").unwrap());
        assert!(!parse_bool("0").unwrap());
        assert!(parse_bool("maybe").is_err());
    }

    #[test]
    fn defaults_are_consistent() {
        let c = Config::defaults();
        assert!(!c.gain.no_record);
        assert_eq!(c.output.default, OutputDefault::Auto);
        for source in [
            c.gain.no_record_source,
            c.output.default_source,
            c.analysis.subsystem_min_size_source,
        ] {
            assert_eq!(source, Source::Default);
        }
    }

    #[test]
    fn load_from_in_memory_store_round_trips() {
        let mut store = Store::open_in_memory().unwrap();
        set(&mut store, "gain.no_record", "true").unwrap();
        set(&mut store, "output.default", "json").unwrap();
        let cfg = Config::load(&store).unwrap();
        assert!(cfg.gain.no_record);
        assert_eq!(cfg.gain.no_record_source, Source::Settings);
        assert_eq!(cfg.output.default, OutputDefault::Json);
        assert_eq!(cfg.output.default_source, Source::Settings);
    }

    #[test]
    fn set_rejects_unknown_key() {
        let mut store = Store::open_in_memory().unwrap();
        assert!(set(&mut store, "nope", "value").is_err());
    }

    #[test]
    fn set_rejects_invalid_enum_value() {
        let mut store = Store::open_in_memory().unwrap();
        let err = set(&mut store, "output.default", "banana").unwrap_err();
        assert!(err.to_string().contains("auto, human, toon, json"));
    }

    #[test]
    fn invalid_stored_value_warns_and_falls_back() {
        // Hand-write an invalid value (simulates DB hand-edit / older
        // binary writing a key newer one renamed).
        let mut store = Store::open_in_memory().unwrap();
        store.set_setting("output.default", "banana").unwrap();
        let cfg = Config::load(&store).unwrap();
        assert_eq!(cfg.output.default, OutputDefault::Auto);
        assert_eq!(cfg.output.default_source, Source::Default);
    }
}
