//! Advisory layer: tells agents when a query is underserving because
//! of language coverage limits, and suggests a `ripgrep` fallback.
//!
//! Always returns `Option<String>`; callers attach to their machine
//! output via `#[serde(skip_serializing_if = "Option::is_none")]` so
//! the happy path emits nothing.

use std::path::Path;

use repoctx_index::{Coverage, Language};

/// Suggest a fallback for a file-targeted command (`outline`,
/// `context`) when the file's language has partial extractor coverage.
pub fn for_file(file_db_path: &str) -> Option<String> {
    let lang = Language::from_path(Path::new(file_db_path))?;
    advise_for_lang(lang, None)
}

/// Suggest a fallback when the user supplied `--lang <slug>` and that
/// language is `Partial`.
pub fn for_lang_filter(lang_slug: Option<&str>, query: Option<&str>) -> Option<String> {
    let slug = lang_slug?;
    let lang = Language::from_slug(slug)?;
    advise_for_lang(lang, query)
}

/// Generic advisory for an empty result against a workspace where some
/// indexed languages are `Partial`. Conservative: only fires when the
/// repo actually contains a partial-coverage language file, and only
/// when the query returned zero hits.
pub fn for_empty_workspace(
    count: usize,
    per_language_files: &[(String, u64)],
    query: Option<&str>,
) -> Option<String> {
    if count > 0 {
        return None;
    }
    let partial_langs: Vec<&str> = per_language_files
        .iter()
        .filter_map(|(slug, files)| {
            if *files == 0 {
                return None;
            }
            let lang = Language::from_slug(slug)?;
            if lang.coverage() == Coverage::Partial {
                Some(lang.slug())
            } else {
                None
            }
        })
        .collect();
    if partial_langs.is_empty() {
        return None;
    }
    let q = query.unwrap_or("<pattern>");
    Some(format!(
        "no hits. Workspace has files in partial-coverage languages ({}) — repoctx may miss nested keys / sections. For exhaustive search consider: rg -n {}",
        partial_langs.join(", "),
        shell_quote(q),
    ))
}

/// A case-insensitive near-miss for a `definition` query: same name
/// modulo ASCII case, in a definition-shaped kind.
pub struct CaseCandidate {
    pub name: String,
    pub path: String,
    pub line: u32, // 1-based, for display
}

/// `definition` is exact-case; `symbols` is case-insensitive. When an
/// exact lookup returns nothing but a case variant exists, a bare
/// `count: 0` reads as "doesn't exist" (our own AGENTS.md rule 4) — a
/// false negative. Surface the variants so the agent retries with the
/// right casing instead of reaching for `grep`.
pub fn for_case_mismatch(query: &str, candidates: &[CaseCandidate]) -> Option<String> {
    if candidates.is_empty() {
        return None;
    }
    let shown: Vec<String> = candidates
        .iter()
        .take(3)
        .map(|c| format!("{} ({}:{})", c.name, c.path, c.line))
        .collect();
    Some(format!(
        "no exact match for '{}'; case-insensitive matches exist: {}. \
         definition is case-sensitive — retry with exact casing or use: repoctx symbols {}",
        query,
        shown.join(", "),
        query,
    ))
}

fn advise_for_lang(lang: Language, query: Option<&str>) -> Option<String> {
    if lang.coverage() == Coverage::Full {
        return None;
    }
    let q = query.unwrap_or("<pattern>");
    Some(format!(
        "{} has partial coverage — {}. For exhaustive search consider: rg -n {}",
        lang.slug(),
        lang.notes(),
        shell_quote(q),
    ))
}

fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".into();
    }
    if s.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/'))
    {
        return s.to_string();
    }
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_language_emits_no_advisory() {
        assert!(for_file("src/main.rs").is_none());
        assert!(for_file("src/lib.ts").is_none());
        assert!(for_file("README.md").is_none());
    }

    #[test]
    fn partial_language_emits_advisory_with_fallback() {
        let a = for_file("config.json").unwrap();
        assert!(a.contains("json"));
        assert!(a.contains("rg -n"));

        let a = for_file("deploy.yaml").unwrap();
        assert!(a.contains("yaml"));

        let a = for_file("pyproject.toml").unwrap();
        assert!(a.contains("toml"));
    }

    #[test]
    fn lang_filter_partial_advises_with_query() {
        let a = for_lang_filter(Some("yaml"), Some("containerPort")).unwrap();
        assert!(a.contains("containerPort"));
        assert!(a.contains("rg -n"));
    }

    #[test]
    fn lang_filter_full_emits_no_advisory() {
        assert!(for_lang_filter(Some("rust"), Some("Foo")).is_none());
    }

    #[test]
    fn empty_workspace_advises_when_partial_files_present() {
        let pl = vec![("rust".into(), 50u64), ("yaml".into(), 3u64)];
        let a = for_empty_workspace(0, &pl, Some("Foo")).unwrap();
        assert!(a.contains("yaml"));
        assert!(a.contains("Foo"));
    }

    #[test]
    fn empty_workspace_silent_when_all_full() {
        let pl = vec![("rust".into(), 50u64), ("python".into(), 10u64)];
        assert!(for_empty_workspace(0, &pl, Some("Foo")).is_none());
    }

    #[test]
    fn non_empty_result_skips_workspace_advisory() {
        let pl = vec![("yaml".into(), 3u64)];
        assert!(for_empty_workspace(2, &pl, Some("Foo")).is_none());
    }

    #[test]
    fn case_mismatch_lists_candidates_and_suggests_symbols() {
        let cands = vec![CaseCandidate {
            name: "Store".into(),
            path: "crates/store/src/store.rs".into(),
            line: 28,
        }];
        let a = for_case_mismatch("store", &cands).unwrap();
        assert!(a.contains("no exact match for 'store'"));
        assert!(a.contains("Store (crates/store/src/store.rs:28)"));
        assert!(a.contains("repoctx symbols store"));
    }

    #[test]
    fn case_mismatch_caps_at_three() {
        let cands: Vec<CaseCandidate> = (0..5)
            .map(|i| CaseCandidate {
                name: format!("V{i}"),
                path: "f.rs".into(),
                line: i + 1,
            })
            .collect();
        let a = for_case_mismatch("v", &cands).unwrap();
        assert_eq!(a.matches("f.rs:").count(), 3);
    }

    #[test]
    fn case_mismatch_empty_is_silent() {
        assert!(for_case_mismatch("x", &[]).is_none());
    }

    #[test]
    fn quote_preserves_simple_identifiers() {
        assert_eq!(shell_quote("Foo"), "Foo");
        assert_eq!(shell_quote("foo_bar"), "foo_bar");
        assert_eq!(shell_quote("a/b.txt"), "a/b.txt");
    }

    #[test]
    fn quote_wraps_metacharacters() {
        assert_eq!(shell_quote("foo bar"), "'foo bar'");
        assert_eq!(shell_quote("a*b"), "'a*b'");
    }
}
