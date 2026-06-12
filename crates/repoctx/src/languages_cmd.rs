//! `repoctx languages` — surface the per-language coverage matrix so
//! agents can decide when to fall back to `ripgrep`.

use anyhow::Result;
use repoctx_index::{Language, ALL_LANGUAGES};
use serde::Serialize;

use crate::output::{HumanRender, Render};

#[derive(Debug, Serialize)]
pub struct LanguageEntry {
    pub slug: String,
    pub coverage: String,
    pub notes: String,
}

#[derive(Debug, Serialize)]
pub struct LanguagesReport {
    pub count: usize,
    pub items: Vec<LanguageEntry>,
}

impl HumanRender for LanguagesReport {
    fn human(&self) -> String {
        let mut out = String::new();
        let w_slug = self.items.iter().map(|i| i.slug.len()).max().unwrap_or(0);
        let w_cov = self
            .items
            .iter()
            .map(|i| i.coverage.len())
            .max()
            .unwrap_or(0);
        for (i, e) in self.items.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&format!(
                "{slug:<w_slug$}  {cov:<w_cov$}  {notes}",
                slug = e.slug,
                cov = e.coverage,
                notes = e.notes,
                w_slug = w_slug,
                w_cov = w_cov,
            ));
        }
        out
    }
}

pub fn run(render: Render) -> Result<()> {
    let items: Vec<LanguageEntry> = ALL_LANGUAGES.iter().map(|l| to_entry(*l)).collect();
    let report = LanguagesReport {
        count: items.len(),
        items,
    };
    crate::output::emit(&report, render)
}

fn to_entry(l: Language) -> LanguageEntry {
    LanguageEntry {
        slug: l.slug().to_string(),
        coverage: l.coverage().slug().to_string(),
        notes: l.notes().to_string(),
    }
}
