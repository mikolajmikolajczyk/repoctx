//! Language registry: extension → grammar + tags query source.
//!
//! All per-language facts live in one source-of-truth table ([`LANGS`]), one
//! row per [`Language`], in enum-discriminant order so accessors are O(1)
//! lookups (`&LANGS[lang as usize]`). Adding a language is one table row; a
//! completeness test (below) guarantees the table stays dense + index-aligned
//! with the enum. Static linking only (ADR-0002) — no dynamic grammar loading.

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

/// Supported source languages for indexing. Discriminants are the index into
/// [`LANGS`] — keep this enum and the table in the same order.
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
    Ruby,
    C,
    Cpp,
    Bash,
    Java,
    CSharp,
    Php,
    Lua,
    Kotlin,
    Swift,
}

/// All supported languages in canonical **display** order (what `repoctx
/// languages` prints). Distinct from the enum-discriminant order of [`LANGS`].
pub const ALL_LANGUAGES: &[Language] = &[
    Language::Rust,
    Language::Go,
    Language::Python,
    Language::TypeScript,
    Language::Tsx,
    Language::JavaScript,
    Language::C,
    Language::Cpp,
    Language::Java,
    Language::CSharp,
    Language::Ruby,
    Language::Php,
    Language::Kotlin,
    Language::Swift,
    Language::Lua,
    Language::Bash,
    Language::Markdown,
    Language::Toml,
    Language::Json,
    Language::Yaml,
];

/// One language's full descriptor — the single source of truth. `lang` must
/// equal the row's position in [`LANGS`] (asserted by the completeness test).
struct LangDef {
    lang: Language,
    /// Stable lowercase slug stored in `files.language`.
    slug: &'static str,
    /// File extensions (no dot) that map to this language.
    exts: &'static [&'static str],
    /// Tree-sitter grammar constructor.
    ts_language: fn() -> TsLanguage,
    /// Tags (symbol) query source.
    tags_query: &'static str,
    /// Deep (all-nesting) tags variant for data languages; `None` otherwise.
    tags_query_deep: Option<&'static str>,
    /// Call-site query (`@callee`); `None` where the call graph isn't wired.
    calls_query: Option<&'static str>,
    /// Import-site query (`@module`); `None` where the import graph isn't wired.
    imports_query: Option<&'static str>,
    coverage: Coverage,
    /// One-line note on what the extractor captures vs misses.
    notes: &'static str,
}

/// The descriptor table — one row per [`Language`], in enum-discriminant order.
const LANGS: &[LangDef] = &[
    LangDef {
        lang: Language::Go,
        slug: "go",
        exts: &["go"],
        ts_language: || tree_sitter_go::LANGUAGE.into(),
        tags_query: tree_sitter_go::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/go-calls.scm")),
        imports_query: Some(include_str!("../queries/go-imports.scm")),
        coverage: Coverage::Full,
        notes: "func / method / type (struct/interface) (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Rust,
        slug: "rust",
        exts: &["rs"],
        ts_language: || tree_sitter_rust::LANGUAGE.into(),
        tags_query: tree_sitter_rust::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/rust-calls.scm")),
        imports_query: Some(include_str!("../queries/rust-imports.scm")),
        coverage: Coverage::Full,
        notes: "struct/enum/union/type → class, trait → interface (upstream tags.scm)",
    },
    LangDef {
        lang: Language::TypeScript,
        slug: "typescript",
        exts: &["ts"],
        ts_language: || tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        tags_query: include_str!("../queries/typescript-tags.scm"),
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/javascript-calls.scm")),
        imports_query: Some(include_str!("../queries/javascript-imports.scm")),
        coverage: Coverage::Full,
        notes:
            "interface, class, function (incl. arrow), method, type, enum (vendored Aider tags.scm)",
    },
    LangDef {
        lang: Language::Tsx,
        slug: "tsx",
        exts: &["tsx"],
        ts_language: || tree_sitter_typescript::LANGUAGE_TSX.into(),
        tags_query: include_str!("../queries/typescript-tags.scm"),
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/javascript-calls.scm")),
        imports_query: Some(include_str!("../queries/javascript-imports.scm")),
        coverage: Coverage::Full,
        notes: "same coverage as TypeScript (vendored Aider tags.scm)",
    },
    LangDef {
        lang: Language::JavaScript,
        slug: "javascript",
        exts: &["js", "jsx", "mjs", "cjs"],
        ts_language: || tree_sitter_javascript::LANGUAGE.into(),
        tags_query: tree_sitter_javascript::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/javascript-calls.scm")),
        imports_query: Some(include_str!("../queries/javascript-imports.scm")),
        coverage: Coverage::Full,
        notes: "class, function (incl. arrow), method (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Python,
        slug: "python",
        exts: &["py"],
        ts_language: || tree_sitter_python::LANGUAGE.into(),
        tags_query: tree_sitter_python::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/python-calls.scm")),
        imports_query: Some(include_str!("../queries/python-imports.scm")),
        coverage: Coverage::Full,
        notes: "def / class (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Json,
        slug: "json",
        exts: &["json"],
        ts_language: || tree_sitter_json::LANGUAGE.into(),
        tags_query: include_str!("../queries/json.scm"),
        tags_query_deep: Some(include_str!("../queries/json-deep.scm")),
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Partial,
        notes: "top-level keys only; nested keys are not surfaced",
    },
    LangDef {
        lang: Language::Yaml,
        slug: "yaml",
        exts: &["yaml", "yml"],
        ts_language: || tree_sitter_yaml::LANGUAGE.into(),
        tags_query: include_str!("../queries/yaml.scm"),
        tags_query_deep: Some(include_str!("../queries/yaml-deep.scm")),
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Partial,
        notes: "top-level keys of each document; nested keys are not surfaced",
    },
    LangDef {
        lang: Language::Toml,
        slug: "toml",
        exts: &["toml"],
        ts_language: || tree_sitter_toml_ng::LANGUAGE.into(),
        tags_query: include_str!("../queries/toml.scm"),
        tags_query_deep: Some(include_str!("../queries/toml-deep.scm")),
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Partial,
        notes: "root pairs + [table] + [[array]] headers; keys inside tables are not surfaced",
    },
    LangDef {
        lang: Language::Markdown,
        slug: "markdown",
        exts: &["md", "markdown"],
        ts_language: || tree_sitter_md::LANGUAGE.into(),
        tags_query: include_str!("../queries/markdown.scm"),
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes: "ATX (#) and setext headings (custom query)",
    },
    LangDef {
        lang: Language::Ruby,
        slug: "ruby",
        exts: &["rb"],
        ts_language: || tree_sitter_ruby::LANGUAGE.into(),
        tags_query: tree_sitter_ruby::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes: "module / class / method / singleton method (upstream tags.scm)",
    },
    LangDef {
        lang: Language::C,
        slug: "c",
        exts: &["c", "h"],
        ts_language: || tree_sitter_c::LANGUAGE.into(),
        tags_query: tree_sitter_c::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/c-calls.scm")),
        imports_query: Some(include_str!("../queries/c-imports.scm")),
        coverage: Coverage::Full,
        notes: "function / struct / typedef / enum (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Cpp,
        slug: "cpp",
        exts: &["cc", "cpp", "cxx", "hpp", "hh", "hxx"],
        ts_language: || tree_sitter_cpp::LANGUAGE.into(),
        tags_query: tree_sitter_cpp::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/cpp-calls.scm")),
        imports_query: Some(include_str!("../queries/cpp-imports.scm")),
        coverage: Coverage::Full,
        notes: "class / struct / function / method / enum (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Bash,
        slug: "bash",
        exts: &["sh", "bash"],
        ts_language: || tree_sitter_bash::LANGUAGE.into(),
        tags_query: include_str!("../queries/bash-tags.scm"),
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Partial,
        notes: "function definitions only; variables/aliases not surfaced",
    },
    LangDef {
        lang: Language::Java,
        slug: "java",
        exts: &["java"],
        ts_language: || tree_sitter_java::LANGUAGE.into(),
        tags_query: tree_sitter_java::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: Some(include_str!("../queries/java-calls.scm")),
        imports_query: Some(include_str!("../queries/java-imports.scm")),
        coverage: Coverage::Full,
        notes: "class / interface / method (upstream tags.scm)",
    },
    LangDef {
        lang: Language::CSharp,
        slug: "csharp",
        exts: &["cs"],
        ts_language: || tree_sitter_c_sharp::LANGUAGE.into(),
        tags_query: tree_sitter_c_sharp::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes: "class / interface / method / struct (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Php,
        slug: "php",
        exts: &["php", "phtml"],
        ts_language: || tree_sitter_php::LANGUAGE_PHP.into(),
        tags_query: tree_sitter_php::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes: "class / interface / function / method / trait (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Lua,
        slug: "lua",
        exts: &["lua"],
        ts_language: || tree_sitter_lua::LANGUAGE.into(),
        tags_query: tree_sitter_lua::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes: "function / local function / method (upstream tags.scm)",
    },
    LangDef {
        lang: Language::Kotlin,
        slug: "kotlin",
        exts: &["kt", "kts"],
        ts_language: || tree_sitter_kotlin_ng::LANGUAGE.into(),
        tags_query: include_str!("../queries/kotlin-tags.scm"),
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes: "class / object / function (vendored minimal query)",
    },
    LangDef {
        lang: Language::Swift,
        slug: "swift",
        exts: &["swift"],
        ts_language: || tree_sitter_swift::LANGUAGE.into(),
        tags_query: tree_sitter_swift::TAGS_QUERY,
        tags_query_deep: None,
        calls_query: None,
        imports_query: None,
        coverage: Coverage::Full,
        notes:
            "struct / protocol / function / method (upstream tags.scm; class names not captured)",
    },
];

/// Number of supported languages = rows in [`LANGS`] = `Language` variants.
pub const LANG_COUNT: usize = LANGS.len();

impl Language {
    /// This language's descriptor row (O(1) by discriminant).
    fn def(self) -> &'static LangDef {
        &LANGS[self as usize]
    }

    /// String slug stored in `files.language` (lowercase, stable).
    pub fn slug(self) -> &'static str {
        self.def().slug
    }

    /// Inverse of [`slug`]. Returns `None` for unknown slugs.
    pub fn from_slug(slug: &str) -> Option<Self> {
        LANGS.iter().find(|d| d.slug == slug).map(|d| d.lang)
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        LANGS.iter().find(|d| d.exts.contains(&ext)).map(|d| d.lang)
    }

    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|s| s.to_str())
            .and_then(Self::from_extension)
    }

    pub fn ts_language(self) -> TsLanguage {
        (self.def().ts_language)()
    }

    pub fn tags_query(self) -> &'static str {
        self.def().tags_query
    }

    /// Call-site query for the static call graph (epic af42572 / ADR-0010).
    /// `Some` for the core-8 languages; `None` for the rest. Captures `@callee`.
    pub fn calls_query(self) -> Option<&'static str> {
        self.def().calls_query
    }

    /// Import-site query for the import / dependency graph (epic #4 /
    /// ADR-0011). `Some` for the core-8 languages; `None` otherwise.
    pub fn imports_query(self) -> Option<&'static str> {
        self.def().imports_query
    }

    /// Deep tags query variant for partial-coverage data languages, capturing
    /// keys at any nesting depth. `None` where the normal query already
    /// captures everything (the `nested_keys` option falls back to it then).
    pub fn tags_query_deep(self) -> Option<&'static str> {
        self.def().tags_query_deep
    }

    /// Coverage rating for symbol extraction. The advisory layer uses this to
    /// tell agents when to fall back to `ripgrep`.
    pub fn coverage(self) -> Coverage {
        self.def().coverage
    }

    /// One-line note describing what the extractor captures vs misses.
    /// Surfaced by `repoctx languages` and by the advisory layer.
    pub fn notes(self) -> &'static str {
        self.def().notes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The table must be dense + index-aligned with the enum: row `i`'s `lang`
    /// is the variant whose discriminant is `i`. This is the single guarantee
    /// that replaces per-`match` exhaustiveness — without it `def()` would
    /// silently return the wrong row.
    #[test]
    fn table_is_index_aligned_and_complete() {
        for (i, d) in LANGS.iter().enumerate() {
            assert_eq!(d.lang as usize, i, "LANGS[{i}] has lang {:?}", d.lang);
        }
        assert_eq!(
            LANG_COUNT,
            ALL_LANGUAGES.len(),
            "table vs display list size"
        );
        // Every display-order language is a real table row (round-trips).
        for &l in ALL_LANGUAGES {
            assert_eq!(l.def().lang, l);
        }
    }

    #[test]
    fn slug_round_trips() {
        for &l in ALL_LANGUAGES {
            assert_eq!(Language::from_slug(l.slug()), Some(l));
        }
    }
}
