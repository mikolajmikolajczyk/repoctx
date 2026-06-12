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
