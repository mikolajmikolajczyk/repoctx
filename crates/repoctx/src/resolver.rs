//! Import-specifier → file resolution (issue #8).
//!
//! Turns the raw specifiers stored in the import graph into indexed file
//! paths so `modules`/`import-cycles`/`boundary` can follow edges across the
//! repo — not just relative `./`/`../` imports but **tsconfig path aliases**
//! (`@adapters/*` → `src/adapters/*`), which dominate real TS codebases.
//!
//! Scope: TS/JS (relative + tsconfig `paths`/`baseUrl` aliases, collected from
//! every `tsconfig*.json` / `jsconfig.json` at the repo root) and **Rust**
//! (intra-crate `crate::`/`self::`/`super::` paths against the crate `src` root,
//! via the `a/b.rs` | `a/b/mod.rs` module-file convention). Bare/package
//! specifiers (`react`), external crates (`std`, `serde`), workspace-member
//! crates, and anything unresolved stay external — never wrong, just
//! unresolved. Python/Go module resolution remains future work.

use std::collections::HashSet;
use std::path::Path;

/// Candidate extensions + `/index` forms tried when a specifier lacks one.
const EXTS: &[&str] = &[
    "", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".d.ts", ".rs", ".py", ".go",
];

/// One tsconfig `paths` mapping. Wildcard form keeps the parts before `*`.
struct Alias {
    /// For `@adapters/*` this is `@adapters/`; for exact `@core` it's `@core`.
    key: String,
    /// baseUrl-joined target before `*` (`src/adapters/`), or the exact target.
    repl: String,
    wildcard: bool,
}

pub struct ImportResolver {
    files: HashSet<String>,
    aliases: Vec<Alias>,
}

impl ImportResolver {
    /// Build from the indexed file set + tsconfig/jsconfig at the repo root.
    pub fn load(repo_root: &Path, files: HashSet<String>) -> Self {
        let aliases = read_aliases(repo_root);
        ImportResolver { files, aliases }
    }

    /// Resolve `spec` imported from `importer` (DB path) to an indexed file,
    /// or `None` (bare/package/unresolved). `importer` != result.
    pub fn resolve(&self, importer: &str, spec: &str) -> Option<String> {
        let target = if spec.starts_with('.') {
            let dir = importer.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
            let joined = if dir.is_empty() {
                spec.to_string()
            } else {
                format!("{dir}/{spec}")
            };
            self.try_candidates(&normalize_path(&joined))?
        } else if importer.ends_with(".rs") {
            self.resolve_rust(importer, spec)?
        } else {
            self.resolve_alias(spec)?
        };
        (target != importer).then_some(target)
    }

    /// Resolve a Rust `use` path to a file within the same crate (issue #8).
    /// Handles intra-crate `crate::` / `self::` / `super::` paths against the
    /// crate's `src` root via the module-file convention (`a/b.rs` or
    /// `a/b/mod.rs`); external crates (`std`, `serde`, workspace members) and
    /// anything unresolved stay external. Item segments are dropped by trying
    /// the longest module prefix first. Heuristic — does not parse `mod`
    /// declarations, so a non-conventional module layout may miss.
    fn resolve_rust(&self, importer: &str, spec: &str) -> Option<String> {
        // Drop a `{...}` import group + any trailing `::`/space.
        let path = spec.split('{').next().unwrap_or(spec);
        let path = path.trim().trim_end_matches([':', ' ', ',']);
        let segs: Vec<&str> = path
            .split("::")
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        let src_root = rust_src_root(importer)?;
        let base: Vec<String> = match *segs.first()? {
            "crate" => segs[1..].iter().map(|s| s.to_string()).collect(),
            "self" | "super" => {
                // Consume the leading run of self (stay) / super (up one module).
                let mut m = importer_mod_dir(importer, &src_root);
                let mut i = 0;
                while let Some(&s) = segs.get(i) {
                    match s {
                        "self" => {}
                        "super" => {
                            m.pop();
                        }
                        _ => break,
                    }
                    i += 1;
                }
                m.extend(segs[i..].iter().map(|s| s.to_string()));
                m
            }
            // std / external crate / workspace member crate (deferred).
            _ => return None,
        };
        // Longest module prefix that maps to a file wins (drops item segments).
        for k in (1..=base.len()).rev() {
            let p = format!("{}/{}", src_root, base[..k].join("/"));
            if let Some(f) = self.try_rust_file(&p) {
                return Some(f);
            }
        }
        None
    }

    /// `base.rs` or `base/mod.rs` against the indexed file set.
    fn try_rust_file(&self, base: &str) -> Option<String> {
        [format!("{base}.rs"), format!("{base}/mod.rs")]
            .into_iter()
            .find(|c| self.files.contains(c))
    }

    fn resolve_alias(&self, spec: &str) -> Option<String> {
        for a in &self.aliases {
            if a.wildcard {
                if let Some(tail) = spec.strip_prefix(&a.key) {
                    let cand = normalize_path(&format!("{}{}", a.repl, tail));
                    if let Some(f) = self.try_candidates(&cand) {
                        return Some(f);
                    }
                }
            } else if spec == a.key {
                if let Some(f) = self.try_candidates(&normalize_path(&a.repl)) {
                    return Some(f);
                }
            }
        }
        None
    }

    /// Try `base`, `base.<ext>`, and `base/index.<ext>` against the file set.
    fn try_candidates(&self, base: &str) -> Option<String> {
        for e in EXTS {
            let c = format!("{base}{e}");
            if self.files.contains(&c) {
                return Some(c);
            }
        }
        for e in EXTS {
            let c = format!("{base}/index{e}");
            if self.files.contains(&c) {
                return Some(c);
            }
        }
        None
    }
}

/// The crate `src` root of a Rust DB path: the prefix up to and including the
/// first `src` segment (`crates/store/src/x.rs` → `crates/store/src`,
/// `src/main.rs` → `src`). `None` if the file isn't under a `src/`.
fn rust_src_root(importer: &str) -> Option<String> {
    if let Some(i) = importer.find("/src/") {
        return Some(importer[..i + 4].to_string());
    }
    if importer.starts_with("src/") {
        return Some("src".to_string());
    }
    None
}

/// Module-directory segments of a Rust file, relative to its crate `src` root,
/// for `self::`/`super::` resolution. `src/foo/bar.rs` → `[foo, bar]`;
/// `lib.rs`/`main.rs`/`mod.rs` collapse to their containing directory.
fn importer_mod_dir(importer: &str, src_root: &str) -> Vec<String> {
    let rel = importer
        .strip_prefix(&format!("{src_root}/"))
        .unwrap_or(importer);
    let rel = rel.strip_suffix(".rs").unwrap_or(rel);
    let mut segs: Vec<String> = rel.split('/').map(|s| s.to_string()).collect();
    if matches!(
        segs.last().map(String::as_str),
        Some("lib" | "main" | "mod")
    ) {
        segs.pop();
    }
    segs
}

/// Collapse `.`/`..`/empty segments in a `/`-separated path.
pub fn normalize_path(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Collect tsconfig `paths` aliases from every `tsconfig*.json` / `jsconfig.json`
/// at the repo root (covers split base/app + extends without chasing chains).
fn read_aliases(repo_root: &Path) -> Vec<Alias> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(repo_root) else {
        return out;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let is_cfg =
            (name.starts_with("tsconfig") && name.ends_with(".json")) || name == "jsconfig.json";
        if !is_cfg {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        parse_aliases(&text, &mut out);
    }
    out
}

/// Parse one (tsconfig) JSON(C) string's `compilerOptions.{baseUrl,paths}`
/// into [`Alias`]es, appending to `out`. Tolerant of comments + trailing
/// commas; silently skips anything it can't parse.
fn parse_aliases(text: &str, out: &mut Vec<Alias>) {
    let cleaned = strip_jsonc(text);
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&cleaned) else {
        return;
    };
    let Some(co) = v.get("compilerOptions") else {
        return;
    };
    let base_url = co.get("baseUrl").and_then(|b| b.as_str()).unwrap_or(".");
    let Some(paths) = co.get("paths").and_then(|p| p.as_object()) else {
        return;
    };
    for (key, vals) in paths {
        let Some(first) = vals
            .as_array()
            .and_then(|a| a.first())
            .and_then(|s| s.as_str())
        else {
            continue;
        };
        let wildcard = key.ends_with("/*") && first.ends_with("/*");
        if wildcard {
            let key_prefix = key.trim_end_matches('*'); // "@adapters/"
            let repl_prefix = first.trim_end_matches('*'); // "src/adapters/"
            out.push(Alias {
                key: key_prefix.to_string(),
                repl: normalize_path(&format!("{base_url}/{repl_prefix}")) + "/",
                wildcard: true,
            });
        } else if !key.contains('*') && !first.contains('*') {
            out.push(Alias {
                key: key.clone(),
                repl: normalize_path(&format!("{base_url}/{first}")),
                wildcard: false,
            });
        }
    }
}

/// Strip `//` line comments, `/* */` block comments, and trailing commas so
/// serde_json can read a tsconfig. Quote-aware so `//` inside a string stays.
fn strip_jsonc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        let c = b[i];
        if in_str {
            out.push(c as char);
            if c == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1] as char);
                i += 2;
                continue;
            }
            if c == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'"' => {
                in_str = true;
                out.push('"');
                i += 1;
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'*' => {
                i += 2;
                while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
            }
            _ => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    // Drop trailing commas before } or ].
    drop_trailing_commas(&out)
}

fn drop_trailing_commas(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let chars: Vec<char> = s.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        if c == ',' {
            let next = chars[i + 1..].iter().find(|c| !c.is_whitespace());
            if matches!(next, Some('}') | Some(']')) {
                continue;
            }
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(list: &[&str]) -> HashSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn relative_resolution() {
        let r = ImportResolver {
            files: files(&["src/ui/util.ts", "src/core/index.ts"]),
            aliases: vec![],
        };
        assert_eq!(
            r.resolve("src/ui/Panel.tsx", "./util"),
            Some("src/ui/util.ts".into())
        );
        assert_eq!(
            r.resolve("src/ui/Panel.tsx", "../core"),
            Some("src/core/index.ts".into())
        );
        assert_eq!(r.resolve("src/ui/Panel.tsx", "react"), None);
    }

    #[test]
    fn tsconfig_alias_resolution() {
        let mut aliases = vec![];
        parse_aliases(
            r#"{ "compilerOptions": { "baseUrl": ".", "paths": {
                "@adapters/*": ["src/adapters/*"],
                "@core": ["src/core/index.ts"]
            } } }"#,
            &mut aliases,
        );
        let r = ImportResolver {
            files: files(&["src/adapters/storage.ts", "src/core/index.ts"]),
            aliases,
        };
        assert_eq!(
            r.resolve("src/ui/a.tsx", "@adapters/storage"),
            Some("src/adapters/storage.ts".into())
        );
        assert_eq!(
            r.resolve("src/ui/a.tsx", "@core"),
            Some("src/core/index.ts".into())
        );
    }

    #[test]
    fn rust_crate_path_resolution() {
        let r = ImportResolver {
            files: files(&[
                "crates/repoctx/src/main.rs",
                "crates/repoctx/src/gain.rs",
                "crates/repoctx/src/output.rs",
                "crates/repoctx/src/read_cmd.rs",
                "crates/repoctx/src/hook/mod.rs",
            ]),
            aliases: vec![],
        };
        let from = "crates/repoctx/src/main.rs";
        // item segment dropped: crate::gain::GainOpts -> gain.rs
        assert_eq!(
            r.resolve(from, "crate::gain::GainOpts"),
            Some("crates/repoctx/src/gain.rs".into())
        );
        // `{...}` group stripped: crate::output::{HumanRender, Render} -> output.rs
        assert_eq!(
            r.resolve(from, "crate::output::{HumanRender, Render}"),
            Some("crates/repoctx/src/output.rs".into())
        );
        // single module segment
        assert_eq!(
            r.resolve(from, "crate::read_cmd"),
            Some("crates/repoctx/src/read_cmd.rs".into())
        );
        // mod.rs form
        assert_eq!(
            r.resolve(from, "crate::hook::install"),
            Some("crates/repoctx/src/hook/mod.rs".into())
        );
        // external crate + std stay external
        assert_eq!(r.resolve(from, "serde::Serialize"), None);
        assert_eq!(r.resolve(from, "std::collections::HashMap"), None);
        assert_eq!(r.resolve(from, "repoctx_store::Store"), None);
    }

    #[test]
    fn rust_self_super_resolution() {
        let r = ImportResolver {
            files: files(&["src/a.rs", "src/a/b.rs", "src/a/helper.rs", "src/top.rs"]),
            aliases: vec![],
        };
        // self:: from a/b.rs -> module a::b, self::x stays in a/b/... ; here
        // super:: drops back to module `a` -> a/helper.rs
        assert_eq!(
            r.resolve("src/a/b.rs", "super::helper::go"),
            Some("src/a/helper.rs".into())
        );
        // super:: from a/b.rs reaching the crate root sibling
        assert_eq!(
            r.resolve("src/a/b.rs", "super::super::top"),
            Some("src/top.rs".into())
        );
    }

    #[test]
    fn jsonc_with_comments_and_trailing_commas() {
        let mut aliases = vec![];
        parse_aliases(
            "{\n // base config\n \"compilerOptions\": {\n \"paths\": {\n \"@x/*\": [\"src/x/*\"], /* trailing */\n },\n },\n}",
            &mut aliases,
        );
        assert_eq!(aliases.len(), 1);
        let r = ImportResolver {
            files: files(&["src/x/y.ts"]),
            aliases,
        };
        assert_eq!(r.resolve("a.ts", "@x/y"), Some("src/x/y.ts".into()));
    }
}
