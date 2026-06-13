//! Per-file Tree-sitter parse + symbol extraction.

use std::sync::OnceLock;

use repoctx_store::SymbolRecord;
use thiserror::Error;
use tracing::debug;
use tree_sitter::{Node, Parser, Query, QueryCursor, StreamingIterator};

use crate::language::Language;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("tree-sitter parser failed to set language for {0:?}")]
    SetLanguage(Language),

    #[error("tree-sitter failed to parse source")]
    Parse,

    #[error("tags query compile failed for {language:?}: {source}")]
    QueryCompile {
        language: Language,
        #[source]
        source: tree_sitter::QueryError,
    },
}

pub type Result<T> = std::result::Result<T, ExtractError>;

/// Per-language compiled query cache.
struct Compiled {
    query: Query,
    /// Capture indices that hold the symbol name. Upstream queries use a
    /// bare `@name`; Aider-vendored queries use dotted `@name.definition.X`.
    /// Either form is treated as a name capture.
    name_idxs: Vec<u32>,
    def_kinds: Vec<(u32, &'static str)>, // (capture index, "function"|"method"|...|"other")
}

/// Options controlling extraction.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParseOptions {
    /// Capture keys at any nesting depth for JSON/YAML/TOML (opt-in). No
    /// effect on languages without a deep query variant.
    pub nested_keys: bool,
}

fn compile(language: Language) -> Result<Compiled> {
    compile_query(language, language.tags_query())
}

fn compile_deep(language: Language) -> Result<Compiled> {
    let q = language
        .tags_query_deep()
        .unwrap_or_else(|| language.tags_query());
    compile_query(language, q)
}

fn compile_query(language: Language, query_src: &str) -> Result<Compiled> {
    let ts_lang = language.ts_language();
    let query = Query::new(&ts_lang, query_src)
        .map_err(|source| ExtractError::QueryCompile { language, source })?;
    let mut name_idxs = Vec::new();
    let mut def_kinds = Vec::new();
    for (i, cap) in query.capture_names().iter().enumerate() {
        let i = i as u32;
        // Name captures: `@name` (upstream) or `@name.<anything>` (Aider).
        if *cap == "name" || cap.starts_with("name.") {
            name_idxs.push(i);
        } else if let Some(rest) = cap.strip_prefix("definition.") {
            def_kinds.push((i, definition_kind(rest)));
        }
    }
    Ok(Compiled {
        query,
        name_idxs,
        def_kinds,
    })
}

/// Map `@definition.<rest>` capture suffix → SymbolKind string slug.
fn definition_kind(rest: &str) -> &'static str {
    match rest {
        "function" => "function",
        "method" => "method",
        "class" => "class",
        "interface" => "interface",
        "module" => "module",
        "constant" => "constant",
        "type" => "type",
        "enum" => "enum",
        "struct" => "struct",
        "trait" => "trait",
        "macro" => "macro",
        "section" => "section",
        "key" => "key",
        _ => "other",
    }
}

/// Look up (and lazily compile) the per-language query.
fn compiled_for(language: Language) -> &'static Result<Compiled> {
    macro_rules! slot {
        ($lang:expr) => {{
            static CELL: OnceLock<Result<Compiled>> = OnceLock::new();
            CELL.get_or_init(|| compile($lang))
        }};
    }
    match language {
        Language::Go => slot!(Language::Go),
        Language::Rust => slot!(Language::Rust),
        Language::TypeScript => slot!(Language::TypeScript),
        Language::Tsx => slot!(Language::Tsx),
        Language::JavaScript => slot!(Language::JavaScript),
        Language::Python => slot!(Language::Python),
        Language::Json => slot!(Language::Json),
        Language::Yaml => slot!(Language::Yaml),
        Language::Toml => slot!(Language::Toml),
        Language::Markdown => slot!(Language::Markdown),
        Language::Ruby => slot!(Language::Ruby),
        Language::C => slot!(Language::C),
        Language::Cpp => slot!(Language::Cpp),
        Language::Bash => slot!(Language::Bash),
        Language::Java => slot!(Language::Java),
        Language::CSharp => slot!(Language::CSharp),
        Language::Php => slot!(Language::Php),
        Language::Lua => slot!(Language::Lua),
        Language::Kotlin => slot!(Language::Kotlin),
        Language::Swift => slot!(Language::Swift),
    }
}

/// Deep (nested-key) compiled query for the data languages that have a
/// deep variant. Falls back to the normal cache for everything else.
fn compiled_deep_for(language: Language) -> &'static Result<Compiled> {
    macro_rules! slot {
        ($lang:expr) => {{
            static CELL: OnceLock<Result<Compiled>> = OnceLock::new();
            CELL.get_or_init(|| compile_deep($lang))
        }};
    }
    match language {
        Language::Json => slot!(Language::Json),
        Language::Yaml => slot!(Language::Yaml),
        Language::Toml => slot!(Language::Toml),
        other => compiled_for(other),
    }
}

/// Parse `source` and extract symbols with default options.
pub fn parse_file(file_path: &str, language: Language, source: &str) -> Result<Vec<SymbolRecord>> {
    parse_file_with(file_path, language, source, ParseOptions::default())
}

/// Parse `source` and extract symbols. `file_path` is the DB-side path
/// stored on each emitted `SymbolRecord`.
pub fn parse_file_with(
    file_path: &str,
    language: Language,
    source: &str,
    opts: ParseOptions,
) -> Result<Vec<SymbolRecord>> {
    let compiled = match if opts.nested_keys {
        compiled_deep_for(language)
    } else {
        compiled_for(language)
    } {
        Ok(c) => c,
        Err(_) => {
            // Compile error: skip the file but don't propagate — keep indexing alive.
            debug!(?language, "tags query failed to compile; skipping");
            return Ok(Vec::new());
        }
    };

    let mut parser = Parser::new();
    parser
        .set_language(&language.ts_language())
        .map_err(|_| ExtractError::SetLanguage(language))?;
    let tree = parser.parse(source, None).ok_or(ExtractError::Parse)?;

    let bytes = source.as_bytes();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&compiled.query, tree.root_node(), bytes);

    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        let mut def_node: Option<Node> = None;
        let mut def_kind: &'static str = "other";
        let mut name_text: Option<String> = None;

        for cap in m.captures {
            if compiled.name_idxs.contains(&cap.index) {
                let raw = node_text(cap.node, bytes);
                name_text = Some(strip_name(raw, language));
            } else if let Some((_, k)) = compiled.def_kinds.iter().find(|(i, _)| *i == cap.index) {
                def_node = Some(cap.node);
                def_kind = k;
            }
        }

        let Some(def) = def_node else { continue };

        let name = if language == Language::Markdown {
            trim_markdown_heading(node_text(def, bytes))
        } else {
            match name_text {
                Some(n) => n,
                None => continue,
            }
        };
        if name.is_empty() {
            continue;
        }

        let start = def.start_position();
        let end = def.end_position();
        out.push(SymbolRecord {
            file_path: file_path.to_string(),
            name,
            kind: def_kind.to_string(),
            start_line: start.row as u32,
            start_column: start.column as u32,
            end_line: end.row as u32,
            end_column: end.column as u32,
        });
    }

    Ok(dedupe_overlaps(out))
}

/// Tree-sitter `tags.scm` files often pair a specific pattern (e.g. method
/// inside an `impl` block) with a more general one (every function). Both
/// fire on the inner node, producing duplicate rows for the same range.
/// Keep the most specific entry per range using a fixed priority.
fn dedupe_overlaps(rows: Vec<SymbolRecord>) -> Vec<SymbolRecord> {
    use std::collections::HashMap;
    fn priority(kind: &str) -> u8 {
        match kind {
            "method" => 10,
            "macro" => 9,
            "constant" => 8,
            "class" | "interface" | "trait" | "struct" | "enum" => 7,
            "module" => 6,
            "type" => 5,
            "field" => 4,
            "variable" => 3,
            "function" => 2,
            "section" | "key" => 1,
            _ => 0,
        }
    }
    let mut by_range: HashMap<(u32, u32, u32, u32), usize> = HashMap::new();
    let mut out: Vec<SymbolRecord> = Vec::with_capacity(rows.len());
    for r in rows {
        let key = (r.start_line, r.start_column, r.end_line, r.end_column);
        match by_range.get(&key) {
            Some(&idx) if priority(&out[idx].kind) >= priority(&r.kind) => {
                // existing wins
            }
            Some(&idx) => {
                out[idx] = r;
            }
            None => {
                by_range.insert(key, out.len());
                out.push(r);
            }
        }
    }
    out
}

fn node_text<'a>(n: Node, bytes: &'a [u8]) -> &'a str {
    n.utf8_text(bytes).unwrap_or("")
}

/// Strip surrounding quotes from string-keyed captures (json, toml).
fn strip_name(raw: &str, language: Language) -> String {
    if matches!(language, Language::Toml | Language::Json | Language::Yaml) {
        let t = raw.trim();
        let t = t
            .strip_prefix('"')
            .unwrap_or(t)
            .strip_suffix('"')
            .unwrap_or(t);
        let t = t
            .strip_prefix('\'')
            .unwrap_or(t)
            .strip_suffix('\'')
            .unwrap_or(t);
        t.to_string()
    } else {
        raw.to_string()
    }
}

/// Trim ATX markers / setext underlines + surrounding whitespace from a
/// raw heading node's source text.
fn trim_markdown_heading(raw: &str) -> String {
    // ATX: drop leading '#'s and optional trailing '#'s; setext: drop the
    // underline line entirely.
    let first_line = raw.lines().next().unwrap_or("").trim();
    let stripped = first_line.trim_start_matches('#').trim();
    let stripped = stripped.trim_end_matches('#').trim();
    stripped.to_string()
}
