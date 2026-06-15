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
                "expected one of [auto, off, force] (got '{other}')"
            )),
        }
    }
}

/// `hook.use_rtk` — whether `repoctx hook claude` chains `rtk hook claude`
/// underneath on passthrough. `Auto` = chain when rtk is on PATH.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookUseRtk {
    Auto,
    On,
    Off,
}

impl HookUseRtk {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::On => "on",
            Self::Off => "off",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "on" => Ok(Self::On),
            "off" => Ok(Self::Off),
            other => Err(anyhow!("expected one of [auto, on, off] (got '{other}')")),
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

/// Split a `,`- or newline-separated list, trimming + dropping blanks.
fn split_list(s: &str) -> Vec<String> {
    s.split([',', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
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
    /// Whether to chain `rtk hook claude` on passthrough.
    pub use_rtk: HookUseRtk,
    pub use_rtk_source: Source,
    /// Allowlist of tools repoctx may chain underneath on passthrough.
    /// Only `rtk` is meaningful in v0.6.0; structural for future tools.
    pub chainable: Vec<String>,
    pub chainable_source: Source,
    /// PreToolUse hooks displaced by `repoctx hook install` so the
    /// runtime handler can chain through them on passthrough. Stored
    /// as a `\n`-separated string in the settings table.
    pub chain_commands: Vec<String>,
    pub chain_commands_source: Source,
    /// Record per-command grep/rg/find passthrough telemetry (issue #7).
    /// Local-only, aggregate (no command bodies); powers `repoctx discover`.
    pub telemetry: bool,
    pub telemetry_source: Source,
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
pub struct IndexConfig {
    /// Capture nested keys in JSON/YAML/TOML (opt-in). Re-index required
    /// after flipping (`repoctx index --force`).
    pub nested_keys: bool,
    pub nested_keys_source: Source,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub hook: HookConfig,
    pub gain: GainConfig,
    pub output: OutputConfig,
    pub index: IndexConfig,
}

impl Config {
    /// Built-in defaults. Used as a starting point if there's no DB to
    /// read from (e.g. tests).
    pub fn defaults() -> Self {
        Self {
            hook: HookConfig {
                rewrite: HookRewrite::Auto,
                rewrite_source: Source::Default,
                use_rtk: HookUseRtk::Auto,
                use_rtk_source: Source::Default,
                chainable: vec!["rtk".to_string()],
                chainable_source: Source::Default,
                chain_commands: Vec::new(),
                chain_commands_source: Source::Default,
                telemetry: true,
                telemetry_source: Source::Default,
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
            index: IndexConfig {
                nested_keys: false,
                nested_keys_source: Source::Default,
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
                "hook.use_rtk" => match HookUseRtk::parse(&value) {
                    Ok(v) => {
                        cfg.hook.use_rtk = v;
                        cfg.hook.use_rtk_source = Source::Settings;
                    }
                    Err(e) => warn_invalid(&key, &value, e),
                },
                // Removed in 0.5.3: install content is embedded in the
                // binary; there is no fetch ref or cache. Old rows are
                // ignored quietly rather than warned about.
                "hook.ref" | "hook.no_cache" => {}
                "hook.chainable" => {
                    cfg.hook.chainable = split_list(&value);
                    cfg.hook.chainable_source = Source::Settings;
                }
                "hook.chain_commands" => {
                    cfg.hook.chain_commands = value
                        .split('\n')
                        .filter(|s| !s.trim().is_empty())
                        .map(str::to_string)
                        .collect();
                    cfg.hook.chain_commands_source = Source::Settings;
                }
                "hook.telemetry" => match parse_bool(&value) {
                    Ok(v) => {
                        cfg.hook.telemetry = v;
                        cfg.hook.telemetry_source = Source::Settings;
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
        if let Ok(v) = env::var("REPOCTX_HOOK_USE_RTK") {
            match HookUseRtk::parse(&v) {
                Ok(u) => {
                    cfg.hook.use_rtk = u;
                    cfg.hook.use_rtk_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_HOOK_USE_RTK", &v, e),
            }
        }
        if let Ok(v) = env::var("REPOCTX_HOOK_TELEMETRY") {
            match parse_bool(&v) {
                Ok(b) => {
                    cfg.hook.telemetry = b;
                    cfg.hook.telemetry_source = Source::Env;
                }
                Err(e) => warn_invalid("REPOCTX_HOOK_TELEMETRY", &v, e),
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
        "hook.use_rtk" => HookUseRtk::parse(value)?.as_str().to_string(),
        "hook.chainable" => split_list(value).join(","),
        "hook.chain_commands" => value.to_string(),
        "hook.telemetry" | "gain.no_record" | "gain.record_query" | "index.nested_keys" => {
            fmt_bool(parse_bool(value)?).to_string()
        }
        "output.default" => OutputDefault::parse(value)?.as_str().to_string(),
        "hook.script_path" => {
            return Err(anyhow!(
                "hook.script_path is read-only (managed by `repoctx init`)"
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
        ("hook.rewrite", HookRewrite::Auto.as_str().to_string()),
        ("hook.use_rtk", HookUseRtk::Auto.as_str().to_string()),
        ("hook.chainable", "rtk".to_string()),
        ("hook.chain_commands", String::new()),
        ("hook.telemetry", fmt_bool(true).to_string()),
        ("gain.no_record", fmt_bool(false).to_string()),
        ("gain.record_query", fmt_bool(false).to_string()),
        ("output.default", OutputDefault::Auto.as_str().to_string()),
        ("index.nested_keys", fmt_bool(false).to_string()),
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
    fn use_rtk_parses() {
        assert_eq!(HookUseRtk::parse("auto").unwrap(), HookUseRtk::Auto);
        assert_eq!(HookUseRtk::parse("ON").unwrap(), HookUseRtk::On);
        assert_eq!(HookUseRtk::parse("off").unwrap(), HookUseRtk::Off);
        assert!(HookUseRtk::parse("maybe").is_err());
    }

    #[test]
    fn chainable_defaults_to_rtk_and_round_trips() {
        let mut store = Store::open_in_memory().unwrap();
        assert_eq!(Config::defaults().hook.chainable, vec!["rtk".to_string()]);
        set(&mut store, "hook.chainable", "rtk, future-tool").unwrap();
        set(&mut store, "hook.use_rtk", "on").unwrap();
        let cfg = Config::load(&store).unwrap();
        assert_eq!(cfg.hook.chainable, vec!["rtk", "future-tool"]);
        assert_eq!(cfg.hook.use_rtk, HookUseRtk::On);
        assert_eq!(cfg.hook.use_rtk_source, Source::Settings);
    }

    #[test]
    fn script_path_is_read_only() {
        let mut store = Store::open_in_memory().unwrap();
        let err = set(&mut store, "hook.script_path", "x").unwrap_err();
        assert!(err.to_string().contains("read-only"));
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
        assert!(!c.gain.no_record);
        assert_eq!(c.output.default, OutputDefault::Auto);
        for source in [
            c.hook.rewrite_source,
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
