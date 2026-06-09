use std::path::PathBuf;

use crate::fingerprint::Fingerprint;
use crate::node::NormalizedNode;

/// A duplicate-detection dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum DetectionDimension {
    /// Language-aware AST comparison of top-level code units.
    Ast,
    /// Language-aware AST comparison of nested code regions.
    SubAst,
    /// Identifier/literal-normalized token-window comparison.
    TokenNormalized,
    /// Whitespace-insensitive raw token-window comparison.
    TokenRaw,
    /// Trimmed, whitespace-normalized line-window comparison.
    Line,
}

impl DetectionDimension {
    /// All supported detection dimensions.
    #[must_use]
    pub const fn all() -> &'static [Self] {
        &[
            Self::Ast,
            Self::SubAst,
            Self::TokenNormalized,
            Self::TokenRaw,
            Self::Line,
        ]
    }
}

impl std::fmt::Display for DetectionDimension {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ast => write!(f, "ast"),
            Self::SubAst => write!(f, "sub_ast"),
            Self::TokenNormalized => write!(f, "token_normalized"),
            Self::TokenRaw => write!(f, "token_raw"),
            Self::Line => write!(f, "line"),
        }
    }
}

/// The kind of code unit extracted from source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize)]
pub enum CodeUnitKind {
    Function,
    Method,
    Closure,
    Class,
    ImplBlock,
    TraitImplBlock,
    // Sub-function kinds
    IfBranch,
    MatchArm,
    LoopBody,
    Block,
    TokenWindow,
    LineWindow,
}

impl std::fmt::Display for CodeUnitKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Function => write!(f, "function"),
            Self::Method => write!(f, "method"),
            Self::Closure => write!(f, "closure"),
            Self::Class => write!(f, "class"),
            Self::ImplBlock => write!(f, "impl block"),
            Self::TraitImplBlock => write!(f, "trait impl block"),
            Self::IfBranch => write!(f, "if branch"),
            Self::MatchArm => write!(f, "match arm"),
            Self::LoopBody => write!(f, "loop body"),
            Self::Block => write!(f, "block"),
            Self::TokenWindow => write!(f, "token window"),
            Self::LineWindow => write!(f, "line window"),
        }
    }
}

/// A unit of code extracted and normalized for duplication analysis.
#[derive(Debug, Clone)]
pub struct CodeUnit {
    pub kind: CodeUnitKind,
    pub name: String,
    pub file: PathBuf,
    pub line_start: usize,
    pub line_end: usize,
    pub signature: NormalizedNode,
    pub body: NormalizedNode,
    pub fingerprint: Fingerprint,
    pub node_count: usize,
    /// For sub-function units, the name of the parent function.
    pub parent_name: Option<String>,
    /// Whether this code unit was identified as test code by the language analyzer.
    pub is_test: bool,
}
