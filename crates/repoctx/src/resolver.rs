//! Import-specifier → file resolution (issue #8).
//!
//! Turns the raw specifiers stored in the import graph into indexed file
//! paths so `modules`/`import-cycles`/`boundary` can follow edges across the
//! repo — not just relative `./`/`../` imports but **tsconfig path aliases**
//! (`@adapters/*` → `src/adapters/*`), which dominate real TS codebases.
//!
//! Scope (this slice): TS/JS. Relative + tsconfig `paths`/`baseUrl`. Aliases
//! are collected from every `tsconfig*.json` / `jsconfig.json` at the repo
//! root (so split base/app configs and `extends` chains are covered without
//! resolving the chain). Bare/package specifiers (`react`) and anything
//! unresolved stay external — never wrong, just unresolved. Rust/Python/Go
//! module resolution remains future work.

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
        } else {
            self.resolve_alias(spec)?
        };
        (target != importer).then_some(target)
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
