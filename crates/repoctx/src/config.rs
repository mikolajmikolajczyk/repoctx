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

/// `hook.rewrite` — kill switch for the future transparent rewrite hook
/// (consumer in v0.5.0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookRewrite {
    Auto,
    Off,
    Force,
}

impl HookRewrite {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Off => "off",
            Self::Force => "force",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "off" => Ok(Self::Off),
            "force" => Ok(Self::Force),
            other => Err(anyhow!(
                "hook.rewrite must be one of [auto, off, force] (got '{other}')"
            )),
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
                "output.default must be one of [auto, human, toon, json] (got '{other}')"
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
pub struct HookConfig {
    pub rewrite: HookRewrite,
    pub rewrite_source: Source,
    pub r#ref: Option<String>,
    pub ref_source: Source,
    pub no_cache: bool,
    pub no_cache_source: Source,
    /// PreToolUse hooks displaced by `repoctx hook install` so the
    /// runtime handler can chain through them on passthrough. Stored
    /// as a `\n`-separated string in the settings table.
    pub chain_commands: Vec<String>,
    pub chain_commands_source: Source,
}

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
pub struct Config {
    pub hook: HookConfig,
    pub gain: GainConfig,
    pub output: OutputConfig,
}

impl Config {
    /// Built-in defaults. Used as a starting point if there's no DB to
    /// read from (e.g. tests).
    pub fn defaults() -> Self {
        Self {
            hook: HookConfig {
                rewrite: HookRewrite::Auto,
                rewrite_source: Source::Default,
                r#ref: None,
                ref_source: Source::Default,
                no_cache: false,
                no_cache_source: Source::Default,
                chain_commands: Vec::new(),
                chain_commands_source: Source::Default,
            },
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
                "hook.rewrite" => match HookRewrite::parse(&value) {
                    Ok(v) => {
                        cfg.hook.rewrite = v;
                        cfg.hook.rewrite_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                "hook.ref" => {
                    cfg.hook.r#ref = Some(value);
                    cfg.hook.ref_source = Source::Settings;
                }
                "hook.chain_commands" => {
                    cfg.hook.chain_commands = value
                        .split('\n')
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_string)
                        .collect();
                    cfg.hook.chain_commands_source = Source::Settings;
                }
                "hook.no_cache" => match parse_bool(&value) {
                    Ok(v) => {
                        cfg.hook.no_cache = v;
                        cfg.hook.no_cache_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
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
                "output.default" => match OutputDefault::parse(&value) {
                    Ok(v) => {
                        cfg.output.default = v;
                        cfg.output.default_source = Source::Settings;
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
        if let Ok(v) = env::var("REPOCTX_HOOK_REWRITE") {
            match HookRewrite::parse(&v) {
                Ok(r) => {
                    cfg.hook.rewrite = r;
                    cfg.hook.rewrite_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_HOOK_REWRITE", &v, e),
            }
        }
        if let Ok(v) = env::var("REPOCTX_HOOK_REF") {
            cfg.hook.r#ref = Some(v);
            cfg.hook.ref_source = Source::Env;
        }
        if let Ok(v) = env::var("REPOCTX_HOOK_NO_CACHE") {
            match parse_bool(&v) {
                Ok(b) => {
                    cfg.hook.no_cache = b;
                    cfg.hook.no_cache_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_HOOK_NO_CACHE", &v, e),
            }
        }
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
        if let Ok(v) = env::var("REPOCTX_OUTPUT_DEFAULT") {
            match OutputDefault::parse(&v) {
                Ok(o) => {
                    cfg.output.default = o;
                    cfg.output.default_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_OUTPUT_DEFAULT", &v, e),
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
        "hook.rewrite" => HookRewrite::parse(value)?.as_str().to_string(),
        "hook.ref" => value.to_string(),
        "hook.chain_commands" => value.to_string(),
        "hook.no_cache" | "gain.no_record" | "gain.record_query" => {
            fmt_bool(parse_bool(value)?).to_string()
        }
        "output.default" => OutputDefault::parse(value)?.as_str().to_string(),
        other => return Err(anyhow!("unknown config key: {other}")),
    };
    store.set_setting(key, &normalized)?;
    Ok(normalized)
}

/// All known config keys with their built-in defaults. Used by
/// `config show` to display every row even when none are stored.
pub fn known_keys() -> Vec<(&'static str, String)> {
    vec![
        ("hook.rewrite", HookRewrite::Auto.as_str().to_string()),
        ("hook.ref", String::new()),
        ("hook.chain_commands", String::new()),
        ("hook.no_cache", fmt_bool(false).to_string()),
        ("gain.no_record", fmt_bool(false).to_string()),
        ("gain.record_query", fmt_bool(false).to_string()),
        ("output.default", OutputDefault::Auto.as_str().to_string()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hook_rewrite_parses_case_insensitive() {
        assert_eq!(HookRewrite::parse("Auto").unwrap(), HookRewrite::Auto);
        assert_eq!(HookRewrite::parse("OFF").unwrap(), HookRewrite::Off);
        assert!(HookRewrite::parse("banana").is_err());
    }

    #[test]
    fn output_default_parses_case_insensitive() {
        assert_eq!(OutputDefault::parse("Auto").unwrap(), OutputDefault::Auto);
        assert_eq!(OutputDefault::parse("JSON").unwrap(), OutputDefault::Json);
        assert!(OutputDefault::parse("xml").is_err());
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
        assert_eq!(c.hook.rewrite, HookRewrite::Auto);
        assert_eq!(c.hook.r#ref, None);
        assert!(!c.hook.no_cache);
        assert!(!c.gain.no_record);
        assert_eq!(c.output.default, OutputDefault::Auto);
        for source in [
            c.hook.rewrite_source,
            c.hook.ref_source,
            c.hook.no_cache_source,
            c.gain.no_record_source,
            c.output.default_source,
        ] {
            assert_eq!(source, Source::Default);
        }
    }

    #[test]
    fn load_from_in_memory_store_round_trips() {
        let mut store = Store::open_in_memory().unwrap();
        set(&mut store, "hook.rewrite", "off").unwrap();
        set(&mut store, "gain.no_record", "true").unwrap();
        set(&mut store, "output.default", "json").unwrap();
        let cfg = Config::load(&store).unwrap();
        assert_eq!(cfg.hook.rewrite, HookRewrite::Off);
        assert_eq!(cfg.hook.rewrite_source, Source::Settings);
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
        let err = set(&mut store, "hook.rewrite", "banana").unwrap_err();
        assert!(err.to_string().contains("auto, off, force"));
    }

    #[test]
    fn invalid_stored_value_warns_and_falls_back() {
        // Hand-write an invalid value (simulates DB hand-edit / older
        // binary writing a key newer one renamed).
        let mut store = Store::open_in_memory().unwrap();
        store.set_setting("hook.rewrite", "banana").unwrap();
        let cfg = Config::load(&store).unwrap();
        assert_eq!(cfg.hook.rewrite, HookRewrite::Auto);
        assert_eq!(cfg.hook.rewrite_source, Source::Default);
    }
}
