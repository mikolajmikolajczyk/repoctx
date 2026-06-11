//! Language registry: extension → grammar + tags query source.

use std::path::Path;

use tree_sitter::Language as TsLanguage;

/// Supported source languages for M0 indexing.
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
            Self::TypeScript | Self::Tsx => tree_sitter_typescript::TAGS_QUERY,
            Self::JavaScript => tree_sitter_javascript::TAGS_QUERY,
            Self::Python => tree_sitter_python::TAGS_QUERY,
            Self::Json => include_str!("../queries/json.scm"),
            Self::Yaml => include_str!("../queries/yaml.scm"),
            Self::Toml => include_str!("../queries/toml.scm"),
            Self::Markdown => include_str!("../queries/markdown.scm"),
        }
    }
}
