//! Per-file Tree-sitter parse + symbol extraction.

use std::sync::OnceLock;

use repoctx_store::{CallRecord, ImportRecord, SymbolRecord};
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
        let visibility = visibility_for(language, &name, def, bytes).to_string();
        out.push(SymbolRecord {
            file_path: file_path.to_string(),
            name,
            kind: def_kind.to_string(),
            start_line: start.row as u32,
            start_column: start.column as u32,
            end_line: end.row as u32,
            end_column: end.column as u32,
            visibility,
        });
    }

    Ok(dedupe_overlaps(out))
}

// ── Call-site extraction (static call graph, epic af42572 / ADR-0010) ──

/// Compiled call-site query: the query plus the capture indices named
/// `@callee`.
struct CompiledCalls {
    query: Query,
    callee_idxs: Vec<u32>,
}

fn compile_calls(language: Language) -> Option<Result<CompiledCalls>> {
    let src = language.calls_query()?;
    let build = || {
        let ts_lang = language.ts_language();
        let query = Query::new(&ts_lang, src)
            .map_err(|source| ExtractError::QueryCompile { language, source })?;
        let callee_idxs = query
            .capture_names()
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == "callee")
            .map(|(i, _)| i as u32)
            .collect();
        Ok(CompiledCalls { query, callee_idxs })
    };
    Some(build())
}

/// Lazily compiled call query per language. `None` for languages without a
/// call query (everything outside the core 8).
fn compiled_calls_for(language: Language) -> Option<&'static Result<CompiledCalls>> {
    macro_rules! slot {
        ($lang:expr) => {{
            static CELL: OnceLock<Option<Result<CompiledCalls>>> = OnceLock::new();
            CELL.get_or_init(|| compile_calls($lang)).as_ref()
        }};
    }
    match language {
        Language::Rust => slot!(Language::Rust),
        Language::Python => slot!(Language::Python),
        Language::JavaScript => slot!(Language::JavaScript),
        Language::TypeScript => slot!(Language::TypeScript),
        Language::Tsx => slot!(Language::Tsx),
        Language::Go => slot!(Language::Go),
        Language::C => slot!(Language::C),
        Language::Cpp => slot!(Language::Cpp),
        Language::Java => slot!(Language::Java),
        _ => None,
    }
}

/// Extract call edges from `source`. Each `@callee` capture becomes one
/// [`CallRecord`] attributed to its nearest enclosing function/method symbol
/// (the caller). `symbols` are the already-extracted symbols for this file,
/// used for that containment lookup. Calls with no enclosing callable (e.g.
/// module top-level) are dropped — they have no caller to attribute.
///
/// Callees are recorded by name only; resolution to symbols happens at query
/// time in the store (name-based, per ADR-0010). Returns `Ok(empty)` for
/// languages without a call query.
pub fn parse_calls_with(
    file_path: &str,
    language: Language,
    source: &str,
    symbols: &[SymbolRecord],
) -> Result<Vec<CallRecord>> {
    let compiled = match compiled_calls_for(language) {
        Some(Ok(c)) => c,
        Some(Err(_)) => {
            debug!(?language, "calls query failed to compile; skipping");
            return Ok(Vec::new());
        }
        None => return Ok(Vec::new()),
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
        for cap in m.captures {
            if !compiled.callee_idxs.contains(&cap.index) {
                continue;
            }
            let callee_name = node_text(cap.node, bytes);
            if callee_name.is_empty() {
                continue;
            }
            let site = cap.node.start_position();
            let Some(caller) = enclosing_callable(cap.node, symbols) else {
                continue;
            };
            out.push(CallRecord {
                file_path: file_path.to_string(),
                caller_name: caller.name.clone(),
                caller_start_line: caller.start_line,
                callee_name: callee_name.to_string(),
                site_line: site.row as u32,
                site_column: site.column as u32,
                resolution: "syntactic".to_string(),
                is_method: is_method_call(cap.node),
            });
        }
    }
    Ok(out)
}

/// Whether a captured callee node is a **receiver-value** method call
/// (`obj.foo()`) as opposed to a free/path call (`foo()`, `Type::foo()`,
/// `ns::foo()`). Receiver-awareness (#9): a method call must not resolve to a
/// free `function` of the same name (`map.set()` is not `fn set()`), so the
/// resolver needs to know which calls carry a receiver value.
///
/// Detected purely from the captured node's parent kind — the per-language call
/// queries already place the callee identifier under a member/field/attribute/
/// selector node for receiver calls, and under a plain/scoped/qualified node for
/// free/path calls. Java has one node (`method_invocation`); a receiver call is
/// one with an `object` field.
fn is_method_call(callee: Node) -> bool {
    let Some(parent) = callee.parent() else {
        return false;
    };
    match parent.kind() {
        // Rust/C/C++ `obj.field()` / `ptr->field()`, JS/TS `obj.prop()`,
        // Python `obj.attr()`, Go `obj.Field()` / `pkg.Field()`.
        "field_expression" | "member_expression" | "attribute" | "selector_expression" => true,
        // Java: `obj.method()` has an `object` field; bare `method()` does not.
        "method_invocation" => parent.child_by_field_name("object").is_some(),
        // Plain `foo()`, Rust `Type::foo()` (scoped_identifier), C++ `ns::foo()`
        // (qualified_identifier): free/path call, no receiver value.
        _ => false,
    }
}

// ── Import-site extraction (import / dependency graph, epic #4 / ADR-0011) ──

/// Compiled import query: the query plus the capture indices named
/// `@module` (the import specifier node).
struct CompiledImports {
    query: Query,
    module_idxs: Vec<u32>,
}

fn compile_imports(language: Language) -> Option<Result<CompiledImports>> {
    let src = language.imports_query()?;
    let build = || {
        let ts_lang = language.ts_language();
        let query = Query::new(&ts_lang, src)
            .map_err(|source| ExtractError::QueryCompile { language, source })?;
        let module_idxs = query
            .capture_names()
            .iter()
            .enumerate()
            .filter(|(_, c)| **c == "module")
            .map(|(i, _)| i as u32)
            .collect();
        Ok(CompiledImports { query, module_idxs })
    };
    Some(build())
}

/// Lazily compiled import query per language. `None` for languages without
/// an import query yet.
fn compiled_imports_for(language: Language) -> Option<&'static Result<CompiledImports>> {
    macro_rules! slot {
        ($lang:expr) => {{
            static CELL: OnceLock<Option<Result<CompiledImports>>> = OnceLock::new();
            CELL.get_or_init(|| compile_imports($lang)).as_ref()
        }};
    }
    match language {
        Language::Rust => slot!(Language::Rust),
        Language::Python => slot!(Language::Python),
        Language::JavaScript => slot!(Language::JavaScript),
        Language::TypeScript => slot!(Language::TypeScript),
        Language::Tsx => slot!(Language::Tsx),
        Language::Go => slot!(Language::Go),
        Language::C => slot!(Language::C),
        Language::Cpp => slot!(Language::Cpp),
        Language::Java => slot!(Language::Java),
        _ => None,
    }
}

/// Extract import edges from `source`. Each `@module` capture becomes one
/// [`ImportRecord`] for `file_path`, with the specifier quotes/brackets
/// stripped. Returns `Ok(empty)` for languages without an import query.
///
/// String-based (no specifier→file resolution); that resolution is deferred
/// to a future resolver writing 'semantic' edges into the same table.
pub fn parse_imports(
    file_path: &str,
    language: Language,
    source: &str,
) -> Result<Vec<ImportRecord>> {
    let compiled = match compiled_imports_for(language) {
        Some(Ok(c)) => c,
        Some(Err(_)) => {
            debug!(?language, "imports query failed to compile; skipping");
            return Ok(Vec::new());
        }
        None => return Ok(Vec::new()),
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
        for cap in m.captures {
            if !compiled.module_idxs.contains(&cap.index) {
                continue;
            }
            let module = strip_module(node_text(cap.node, bytes));
            if module.is_empty() {
                continue;
            }
            let site = cap.node.start_position();
            out.push(ImportRecord {
                file_path: file_path.to_string(),
                module,
                site_line: site.row as u32,
                site_column: site.column as u32,
                resolution: "syntactic".to_string(),
            });
        }
    }
    Ok(out)
}

/// Strip surrounding quotes / angle-brackets and whitespace from a captured
/// import-specifier node (`"foo"`, `'foo'`, `<stdio.h>` → `foo` / `stdio.h`).
fn strip_module(raw: &str) -> String {
    raw.trim()
        .trim_matches(|c| matches!(c, '"' | '\'' | '<' | '>'))
        .trim()
        .to_string()
}

/// Node kinds that introduce a callable across the core-8 grammars. Used to
/// find a call site's enclosing function/method by walking up the tree —
/// more robust than symbol line ranges, since some tags queries (C/C++)
/// capture only the declarator line, not the whole body.
fn is_caller_def_kind(kind: &str) -> bool {
    matches!(
        kind,
        "function_item"            // Rust
            | "function_definition" // Python, C, C++
            | "function_declaration" // Go, JS, TS
            | "method_definition"   // JS, TS
            | "method_declaration"  // Go, Java
            | "constructor_declaration" // Java
    )
}

/// Nearest enclosing function/method of a call site: walk up from the call
/// node to the first callable-def ancestor, then match it to a symbol by
/// start line (and function/method kind). Returns `None` for calls with no
/// enclosing callable (e.g. module top-level).
fn enclosing_callable<'a>(
    call_node: Node,
    symbols: &'a [SymbolRecord],
) -> Option<&'a SymbolRecord> {
    let mut cur = Some(call_node);
    while let Some(node) = cur {
        if is_caller_def_kind(node.kind()) {
            let row = node.start_position().row as u32;
            if let Some(s) = symbols
                .iter()
                .find(|s| s.start_line == row && matches!(s.kind.as_str(), "function" | "method"))
            {
                return Some(s);
            }
        }
        cur = node.parent();
    }
    None
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

/// Lexical visibility of a symbol (issue #10). Per-language, syntactic.
/// `"unknown"` where the language has no cheap signal yet — a safe default
/// that preserves prior behavior (the `languages` full/partial philosophy).
///
/// - Go: exported iff the identifier's first letter is uppercase.
/// - Rust: exported iff the item carries any `pub*` modifier (over-approximated
///   — `pub`, `pub(crate)`, `pub(super)`, `pub(in …)` all count; the `pub mod`
///   chain isn't traced), or it's an FFI entry point (`extern` fn,
///   `#[no_mangle]`, `#[export_name]`) which is never dead.
/// - JS/TS/TSX: exported iff the definition is wrapped by an inline
///   `export` (`export function f`, `export class C`, `export const x = …`,
///   `export default …`). `export {{ a }}` clauses + CJS are not handled yet
///   → those stay `private` (a future #10 step); inline export already cuts
///   the common factory false positives.
fn visibility_for(language: Language, name: &str, def: Node, bytes: &[u8]) -> &'static str {
    match language {
        Language::Go => {
            if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                "public"
            } else {
                "private"
            }
        }
        Language::Rust => rust_visibility(def, bytes),
        Language::TypeScript | Language::Tsx | Language::JavaScript => {
            if is_inline_exported(def) {
                "public"
            } else {
                "private"
            }
        }
        _ => "unknown",
    }
}

/// Rust lexical visibility (#10). `pub*` modifier or FFI export → `"public"`,
/// else `"private"`. Over-approximate by design — don't chase the `pub mod`
/// re-export chain; a `pub fn` in a private module still reads as public, which
/// is the safe side for `deadcode` (won't flag a maybe-API symbol).
fn rust_visibility(def: Node, bytes: &[u8]) -> &'static str {
    let mut c = def.walk();
    for ch in def.children(&mut c) {
        match ch.kind() {
            "visibility_modifier" => return "public",
            // `extern "C" fn …` — an FFI export, never dead code.
            "function_modifiers" if node_text(ch, bytes).contains("extern") => return "public",
            _ => {}
        }
    }
    // `#[no_mangle]` / `#[export_name]` attributes precede the item.
    let mut sib = def.prev_sibling();
    let mut hops = 0;
    while let Some(s) = sib {
        match s.kind() {
            "attribute_item" => {
                let t = node_text(s, bytes);
                if t.contains("no_mangle") || t.contains("export_name") {
                    return "public";
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        hops += 1;
        if hops > 8 {
            break;
        }
        sib = s.prev_sibling();
    }
    "private"
}

/// Walk up from a JS/TS definition node looking for the `export_statement`
/// that directly wraps it. Stops at a nesting boundary (a body / another
/// callable / class) so a function nested inside an exported one isn't
/// mistaken for exported. Bounded hop count for safety.
fn is_inline_exported(def: Node) -> bool {
    let mut cur = def.parent();
    let mut hops = 0;
    while let Some(n) = cur {
        match n.kind() {
            "export_statement" => return true,
            // structural wrappers between a `const`/`let` decl and its export.
            "lexical_declaration" | "variable_declaration" | "variable_declarator" => {}
            // anything else (a body, another callable, class, etc.) = not a
            // direct top-level export.
            _ => return false,
        }
        hops += 1;
        if hops > 4 {
            return false;
        }
        cur = n.parent();
    }
    false
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

#[cfg(test)]
mod import_tests {
    use super::*;

    fn modules(language: Language, src: &str) -> Vec<String> {
        parse_imports("f", language, src)
            .unwrap()
            .into_iter()
            .map(|r| r.module)
            .collect()
    }

    #[test]
    fn typescript_esm_imports() {
        let src = r#"
import { saveFile } from "@adapters/storage-idb";
import type { Manifest } from "@adapters/storage-idb";
export { foo } from "./util";
"#;
        assert_eq!(
            modules(Language::TypeScript, src),
            ["@adapters/storage-idb", "@adapters/storage-idb", "./util"]
        );
    }

    #[test]
    fn python_imports_dotted_and_relative() {
        let src = "import os\nimport a.b.c\nfrom x.y import z\n";
        assert_eq!(modules(Language::Python, src), ["os", "a.b.c", "x.y"]);
    }

    #[test]
    fn rust_use_and_extern_crate() {
        let src = "use std::collections::HashMap;\nuse crate::foo::Bar;\nextern crate serde;\n";
        assert_eq!(
            modules(Language::Rust, src),
            ["std::collections::HashMap", "crate::foo::Bar", "serde"]
        );
    }

    #[test]
    fn c_include_strips_quotes_and_brackets() {
        let src = "#include <stdio.h>\n#include \"local.h\"\n";
        assert_eq!(modules(Language::C, src), ["stdio.h", "local.h"]);
    }

    #[test]
    fn uncovered_language_yields_nothing() {
        // Ruby has no import query yet — no-op, not an error.
        assert!(modules(Language::Ruby, "require 'foo'\n").is_empty());
    }

    #[test]
    fn go_visibility_from_capitalization() {
        let syms = parse_file(
            "f.go",
            Language::Go,
            "package p\nfunc Exported() {}\nfunc private() {}\n",
        )
        .unwrap();
        let vis = |n: &str| {
            syms.iter()
                .find(|s| s.name == n)
                .map(|s| s.visibility.as_str())
        };
        assert_eq!(vis("Exported"), Some("public"));
        assert_eq!(vis("private"), Some("private"));
    }

    #[test]
    fn rust_visibility_pub_extern_ffi() {
        let src = "\
pub fn exported() {}
pub(crate) fn crate_pub() {}
fn private_fn() {}
pub struct Thing;
struct Hidden;
#[no_mangle]
pub extern \"C\" fn ffi_entry() {}
#[export_name = \"x\"]
fn named() {}
";
        let syms = parse_file("a.rs", Language::Rust, src).unwrap();
        let vis = |n: &str| syms.iter().find(|s| s.name == n).map(|s| s.visibility.as_str());
        assert_eq!(vis("exported"), Some("public"), "pub");
        assert_eq!(vis("crate_pub"), Some("public"), "pub(crate) over-approximated");
        assert_eq!(vis("private_fn"), Some("private"));
        assert_eq!(vis("Thing"), Some("public"), "pub struct");
        assert_eq!(vis("Hidden"), Some("private"));
        assert_eq!(vis("ffi_entry"), Some("public"), "extern/no_mangle");
        assert_eq!(vis("named"), Some("public"), "#[export_name]");
    }

    #[test]
    fn unsignalled_language_visibility_is_unknown() {
        // A language with no visibility extractor yet keeps the safe default.
        let syms = parse_file("a.c", Language::C, "int foo(void){return 0;}\n").unwrap();
        assert!(syms.iter().all(|s| s.visibility == "unknown"));
    }

    #[test]
    fn ts_inline_export_visibility() {
        let src = "\
export function createFoo() {}
export const createBar = () => {};
function deadHelper() {}
export default function entry() {}
function outer() { function inner() {} }
";
        let syms = parse_file("a.ts", Language::TypeScript, src).unwrap();
        let vis = |n: &str| {
            syms.iter()
                .find(|s| s.name == n)
                .map(|s| s.visibility.as_str())
        };
        assert_eq!(vis("createFoo"), Some("public"));
        assert_eq!(vis("createBar"), Some("public"), "export const arrow");
        assert_eq!(vis("entry"), Some("public"), "export default");
        assert_eq!(vis("deadHelper"), Some("private"));
        // nested fn inside a non-exported outer is NOT exported.
        assert_eq!(vis("inner"), Some("private"));
        assert_eq!(vis("outer"), Some("private"));
    }

    /// Map callee name -> is_method for a parsed snippet (receiver-awareness #9).
    fn call_methods(file: &str, language: Language, src: &str) -> Vec<(String, bool)> {
        let syms = parse_file(file, language, src).unwrap();
        parse_calls_with(file, language, src, &syms)
            .unwrap()
            .into_iter()
            .map(|c| (c.callee_name, c.is_method))
            .collect()
    }

    #[test]
    fn is_method_js_member_vs_plain() {
        let src = "function f() { plain(); obj.member(); a.b.deep(); }\n";
        let calls = call_methods("a.js", Language::JavaScript, src);
        let m = |n: &str| calls.iter().find(|(c, _)| c == n).map(|(_, b)| *b);
        assert_eq!(m("plain"), Some(false), "free call");
        assert_eq!(m("member"), Some(true), "obj.member()");
        assert_eq!(m("deep"), Some(true), "a.b.deep()");
    }

    #[test]
    fn is_method_python_attribute_vs_plain() {
        let src = "def f():\n    plain()\n    obj.method()\n";
        let calls = call_methods("a.py", Language::Python, src);
        let m = |n: &str| calls.iter().find(|(c, _)| c == n).map(|(_, b)| *b);
        assert_eq!(m("plain"), Some(false));
        assert_eq!(m("method"), Some(true));
    }

    #[test]
    fn is_method_rust_field_vs_path() {
        // `Type::assoc()` (scoped) is a free/path call; `x.field()` is a method.
        let src = "fn f() { free(); Type::assoc(); x.field(); }\n";
        let calls = call_methods("a.rs", Language::Rust, src);
        let m = |n: &str| calls.iter().find(|(c, _)| c == n).map(|(_, b)| *b);
        assert_eq!(m("free"), Some(false));
        assert_eq!(m("assoc"), Some(false), "Type::assoc() is a path call");
        assert_eq!(m("field"), Some(true), "x.field() is a method call");
    }

    #[test]
    fn is_method_go_selector() {
        let src = "package p\nfunc f() {\n\tplain()\n\tobj.Method()\n}\n";
        let calls = call_methods("a.go", Language::Go, src);
        let m = |n: &str| calls.iter().find(|(c, _)| c == n).map(|(_, b)| *b);
        assert_eq!(m("plain"), Some(false));
        assert_eq!(m("Method"), Some(true));
    }
}
