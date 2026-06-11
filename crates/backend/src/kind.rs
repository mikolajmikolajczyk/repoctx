use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Closed enum of symbol kinds emitted by any backend.
///
/// Serializes as lowercase. Some variants are unreachable from upstream
/// `tags.scm` queries today; they are kept for custom-query growth and a
/// stable public contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SymbolKind {
    Function,
    Method,
    Class,
    Struct,
    Enum,
    Interface,
    Trait,
    Module,
    Constant,
    Type,
    Variable,
    Field,
    Macro,
    Section,
    Key,
    Other,
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Method => "method",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Enum => "enum",
            Self::Interface => "interface",
            Self::Trait => "trait",
            Self::Module => "module",
            Self::Constant => "constant",
            Self::Type => "type",
            Self::Variable => "variable",
            Self::Field => "field",
            Self::Macro => "macro",
            Self::Section => "section",
            Self::Key => "key",
            Self::Other => "other",
        }
    }
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownKindError(pub String);

impl fmt::Display for UnknownKindError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown SymbolKind: {}", self.0)
    }
}

impl std::error::Error for UnknownKindError {}

impl FromStr for SymbolKind {
    type Err = UnknownKindError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "function" => Self::Function,
            "method" => Self::Method,
            "class" => Self::Class,
            "struct" => Self::Struct,
            "enum" => Self::Enum,
            "interface" => Self::Interface,
            "trait" => Self::Trait,
            "module" => Self::Module,
            "constant" => Self::Constant,
            "type" => Self::Type,
            "variable" => Self::Variable,
            "field" => Self::Field,
            "macro" => Self::Macro,
            "section" => Self::Section,
            "key" => Self::Key,
            "other" => Self::Other,
            _ => return Err(UnknownKindError(s.to_string())),
        })
    }
}
