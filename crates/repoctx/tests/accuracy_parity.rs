//! Accuracy parity vs ripgrep (issue c23894f).
//!
//! For each language fixture with a known symbol inventory (sidecar
//! `expected.toml`), assert repoctx makes no accuracy errors:
//!
//! - zero false negatives: every real definition is found by `symbols`,
//!   and by `definition` when its kind is in the definition whitelist;
//! - zero false positives from text: identifiers that appear only in
//!   comments/strings (or non-indexed kinds like consts) return nothing;
//! - case semantics: `definition` is exact-case, `symbols` is
//!   case-insensitive substring;
//! - partial-coverage languages (JSON/YAML/TOML): `outline` surfaces
//!   every top-level key and attaches the coverage advisory.
//!
//! Ripgrep is the external ground-truth cross-check: every sidecar
//! definition must also be findable by `rg` in its file (proving the
//! fixture really contains it, so repoctx finding it is genuine parity —
//! not both tools coming up empty). The cross-check is skipped if `rg`
//! is not on PATH; the repoctx assertions always run.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct Expected {
    full: BTreeMap<String, FullLang>,
    partial: BTreeMap<String, PartialLang>,
}

#[derive(Deserialize)]
struct FullLang {
    symbols: Vec<Sym>,
    #[serde(default)]
    ghosts: Vec<String>,
}

#[derive(Deserialize)]
struct Sym {
    name: String,
    file: String,
    def: bool,
}

#[derive(Deserialize)]
struct PartialLang {
    file: String,
    keys: Vec<String>,
}

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/parity")
}

fn load_expected() -> Expected {
    let text = std::fs::read_to_string(fixtures_root().join("expected.toml")).unwrap();
    toml::from_str(&text).expect("parse expected.toml")
}

/// Copy a language fixture dir's source files (flat) into `dst`. The
/// sidecar lives outside these dirs, so nothing extraneous is indexed.
fn copy_fixture(lang: &str, dst: &Path) {
    let src = fixtures_root().join(lang);
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        if entry.file_type().unwrap().is_file() {
            std::fs::copy(entry.path(), dst.join(entry.file_name())).unwrap();
        }
    }
}

fn repoctx_json(repo: &Path, args: &[&str]) -> Value {
    let out = Command::cargo_bin("repoctx")
        .unwrap()
        .env("RUST_REPOCTX_NO_RECORD", "1")
        .current_dir(repo)
        .arg("--repo")
        .arg(repo)
        .arg("--json")
        .args(args)
        .output()
        .expect("run repoctx");
    serde_json::from_slice(&out.stdout).unwrap_or_else(|_| {
        panic!(
            "repoctx {:?} produced non-JSON: stdout={} stderr={}",
            args,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr),
        )
    })
}

fn items(v: &Value) -> &[Value] {
    v["items"].as_array().map(|a| a.as_slice()).unwrap_or(&[])
}

/// An item with this exact name whose path ends with `file`.
fn has(v: &Value, name: &str, file: &str) -> bool {
    items(v).iter().any(|i| {
        i["name"].as_str() == Some(name)
            && i["location"]["path"]
                .as_str()
                .is_some_and(|p| p.ends_with(file))
    })
}

/// Any item with this exact name.
fn has_name(v: &Value, name: &str) -> bool {
    items(v).iter().any(|i| i["name"].as_str() == Some(name))
}

fn item_names(v: &Value) -> Vec<String> {
    items(v)
        .iter()
        .filter_map(|i| i["name"].as_str().map(str::to_string))
        .collect()
}

fn rg_available() -> bool {
    StdCommand::new("rg")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// rg ground truth: the identifier literally occurs in `file`.
fn rg_finds(repo: &Path, name: &str, file: &str) -> bool {
    StdCommand::new("rg")
        .current_dir(repo)
        .args(["-n", "--fixed-strings", name, file])
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

#[test]
fn accuracy_parity_full_coverage_languages() {
    let expected = load_expected();
    let rg = rg_available();
    assert!(
        expected.full.len() >= 7,
        "expected >= 7 full-coverage languages, got {}",
        expected.full.len()
    );

    for (lang, fl) in &expected.full {
        let tmp = tempfile::tempdir().unwrap();
        copy_fixture(lang, tmp.path());

        for s in &fl.symbols {
            // Zero false negative on the substring surface.
            let v = repoctx_json(tmp.path(), &["symbols", &s.name]);
            assert!(
                has(&v, &s.name, &s.file),
                "[{lang}] symbols missed `{}` in {}",
                s.name,
                s.file
            );

            // Definition surface for whitelist kinds.
            if s.def {
                let v = repoctx_json(tmp.path(), &["definition", &s.name]);
                assert!(
                    has(&v, &s.name, &s.file),
                    "[{lang}] definition missed `{}` in {}",
                    s.name,
                    s.file
                );
            }

            // Ground-truth cross-check: rg must also find it (fixture is real).
            if rg {
                assert!(
                    rg_finds(tmp.path(), &s.name, &s.file),
                    "[{lang}] rg could not find `{}` in {} — fixture/sidecar drift",
                    s.name,
                    s.file
                );
            }
        }

        // Ghosts: comment/string-only or non-indexed kinds → no hits.
        for g in &fl.ghosts {
            let v = repoctx_json(tmp.path(), &["symbols", g]);
            assert!(
                !has_name(&v, g),
                "[{lang}] symbols false-positive on ghost `{g}`"
            );
            let v = repoctx_json(tmp.path(), &["definition", g]);
            assert!(
                !has_name(&v, g),
                "[{lang}] definition false-positive on ghost `{g}`"
            );
        }
    }
}

#[test]
fn case_sensitivity_definition_vs_symbols() {
    // `Widget` (rust) — definition is exact-case, symbols is not.
    let tmp = tempfile::tempdir().unwrap();
    copy_fixture("rust", tmp.path());

    let lower = repoctx_json(tmp.path(), &["definition", "widget"]);
    assert_eq!(lower["count"], 0, "definition must be case-sensitive");
    assert!(!has_name(&lower, "Widget"));
    // The case-mismatch advisory should point the agent at the right casing.
    assert!(
        lower["advisory"]
            .as_str()
            .is_some_and(|a| a.contains("Widget")),
        "expected case-mismatch advisory naming Widget"
    );

    let any_case = repoctx_json(tmp.path(), &["symbols", "widget"]);
    assert!(
        has_name(&any_case, "Widget"),
        "symbols must match case-insensitively"
    );
}

#[test]
fn partial_coverage_languages_surface_top_keys_with_advisory() {
    let expected = load_expected();
    assert!(expected.partial.len() >= 3, "expected json/yaml/toml");

    for (lang, p) in &expected.partial {
        let tmp = tempfile::tempdir().unwrap();
        copy_fixture(lang, tmp.path());

        let v = repoctx_json(tmp.path(), &["outline", &p.file]);
        assert!(
            v.get("advisory").and_then(Value::as_str).is_some(),
            "[{lang}] outline must attach a coverage advisory"
        );
        let names = item_names(&v);
        for k in &p.keys {
            assert!(
                names.contains(k),
                "[{lang}] outline missing top-level key `{k}` (got {names:?})"
            );
        }
    }
}
