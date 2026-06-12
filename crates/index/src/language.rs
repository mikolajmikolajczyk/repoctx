//! Language registry: extension → grammar + tags query source.

use std::path::Path;

use tree_sitter::Language as TsLanguage;

/// Coverage rating for the symbol-extraction quality of each language.
///
/// Used by the CLI's advisory layer to tell agents when to fall back to
/// `ripgrep`: queries against `Partial` languages may underperform
/// because the extractor misses common constructs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// Every common construct in the language has a tagged capture.
    Full,
    /// Only a subset captured; common queries may miss. Documented in
    /// the per-language `notes`.
    Partial,
}

impl Coverage {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Partial => "partial",
        }
    }
}

/// Supported source languages for indexing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Go,
    Rust,
    TypeScript,
    Tsx,
    JavaScript,
    Python,
    Json,
    Yaml,
    Toml,
    Markdown,
}

/// All supported languages in canonical display order.
pub const ALL_LANGUAGES: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Python,
    Language::TypeScript,
    Language::Tsx,
    Language::JavaScript,
    Language::Markdown,
    Language::Toml,
    Language::Json,
    Language::Yaml,
];

impl Language {
    /// String slug stored in `files.language` (lowercase, stable).
    pub fn slug(self) -> &'static str {
        match self {
            Self::Go => "go",
            Self::Rust => "rust",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Python => "python",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
            Self::Markdown => "markdown",
        }
    }

    /// Inverse of [`slug`]. Returns `None` for unknown slugs.
    pub fn from_slug(slug: &str) -> Option<Self> {
        Some(match slug {
            "go" => Self::Go,
            "rust" => Self::Rust,
            "typescript" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "javascript" => Self::JavaScript,
            "python" => Self::Python,
            "json" => Self::Json,
            "yaml" => Self::Yaml,
            "toml" => Self::Toml,
            "markdown" => Self::Markdown,
            _ => return None,
        })
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        Some(match ext {
            "go" => Self::Go,
            "rs" => Self::Rust,
            "ts" => Self::TypeScript,
            "tsx" => Self::Tsx,
            "js" | "jsx" | "mjs" | "cjs" => Self::JavaScript,
            "py" => Self::Python,
            "json" => Self::Json,
            "yaml" | "yml" => Self::Yaml,
            "toml" => Self::Toml,
            "md" | "markdown" => Self::Markdown,
            _ => return None,
        })
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|s| s.to_str())
            .and_then(Self::from_extension)
    }

    pub fn ts_language(self) -> TsLanguage {
        match self {
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::Json => tree_sitter_json::LANGUAGE.into(),
            Self::Yaml => tree_sitter_yaml::LANGUAGE.into(),
            Self::Toml => tree_sitter_toml_ng::LANGUAGE.into(),
            Self::Markdown => tree_sitter_md::LANGUAGE.into(),
        }
    }

    pub fn tags_query(self) -> &'static str {
        match self {
            Self::Go => tree_sitter_go::TAGS_QUERY,
            Self::Rust => tree_sitter_rust::TAGS_QUERY,
            Self::TypeScript | Self::Tsx => include_str!("../queries/typescript-tags.scm"),
            Self::JavaScript => tree_sitter_javascript::TAGS_QUERY,
            Self::Python => tree_sitter_python::TAGS_QUERY,
            Self::Json => include_str!("../queries/json.scm"),
            Self::Yaml => include_str!("../queries/yaml.scm"),
            Self::Toml => include_str!("../queries/toml.scm"),
            Self::Markdown => include_str!("../queries/markdown.scm"),
        }
    }

    /// Coverage rating for the symbol-extraction quality on this
    /// language. The advisory layer uses this to tell agents when to
    /// fall back to `ripgrep`.
    pub fn coverage(self) -> Coverage {
        match self {
            Self::Go
            | Self::Rust
            | Self::TypeScript
            | Self::Tsx
            | Self::JavaScript
            | Self::Python
            | Self::Markdown => Coverage::Full,
            // JSON/YAML/TOML: only top-level keys (or section headers
            // for TOML) are captured. Nested config-key searches miss.
            // See issue `2c47040` for the opt-in nested-key plan.
            Self::Json | Self::Yaml | Self::Toml => Coverage::Partial,
        }
    }

    /// One-line note describing what the extractor captures vs misses.
    /// Surfaced by `repoctx languages` and by the advisory layer.
    pub fn notes(self) -> &'static str {
        match self {
            Self::Rust => "struct/enum/union/type → class, trait → interface (upstream tags.scm)",
            Self::Go => "func / method / type (struct/interface) (upstream tags.scm)",
            Self::Python => "def / class (upstream tags.scm)",
            Self::TypeScript => {
                "interface, class, function (incl. arrow), method, type, enum (vendored Aider tags.scm)"
            }
            Self::Tsx => {
                "same coverage as TypeScript (vendored Aider tags.scm)"
            }
            Self::JavaScript => "class, function (incl. arrow), method (upstream tags.scm)",
            Self::Markdown => "ATX (#) and setext headings (custom query)",
            Self::Json => "top-level keys only; nested keys are not surfaced",
            Self::Yaml => {
                "top-level keys of each document; nested keys are not surfaced"
            }
            Self::Toml => {
                "root pairs + [table] + [[array]] headers; keys inside tables are not surfaced"
            }
        }
    }
}
