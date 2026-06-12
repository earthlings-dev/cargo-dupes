//! Suppression-rule registry.
//!
//! Every noise judgment the detector makes is a named rule. `Suppress` rules
//! tag candidate units/groups that stay out of the default report; `Admit`
//! rules are named carve-outs that turn otherwise-rejected window shapes into
//! visible candidates. Rules are toggled via `[suppress]` config and the
//! `--disable-rule`/`--enable-rule` CLI flags; disabling a `Suppress` rule
//! makes its candidates fully visible, disabling an `Admit` rule reverts its
//! windows to their base suppression.

use std::collections::BTreeSet;

use crate::code_unit::DetectionDimension;

/// Stable identity of a suppression or admission rule.
///
/// The string form is `"<scope>.<shape>"` via [`RuleId::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RuleId {
    AstSetterReturningSelf,
    AstForwardingAccessor,
    AstBooleanProjection,
    AstComparatorAdapter,
    SubNoStructure,
    SubTrivialPredicate,
    SubEmptyDefaultReturn,
    SubMessageOnlyMacro,
    SubValuePlumbing,
    SubCoveredByChain,
    TokenImportScaffold,
    TokenChainTail,
    TokenSignaturePrefix,
    TokenDeclarationScaffold,
    TokenMatchTablePrefix,
    TokenLowSignal,
    LineImportScaffold,
    LineChainTail,
    LineDeclarationSignaturePrefix,
    LineLowSignal,
    LineDeclarationStanza,
    LineBuilderChainRun,
    GroupCoveredByAst,
    GroupOverlapContained,
}

/// Whether a rule tags individual units or whole groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleLevel {
    Unit,
    Group,
}

/// Whether a rule suppresses candidates or admits otherwise-rejected ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleAction {
    Suppress,
    Admit,
}

/// One registry row.
pub struct SuppressionRule {
    pub id: RuleId,
    pub level: RuleLevel,
    pub action: RuleAction,
    pub dimensions: &'static [DetectionDimension],
    pub description: &'static str,
}

const TOKEN_DIMENSIONS: &[DetectionDimension] = &[
    DetectionDimension::TokenNormalized,
    DetectionDimension::TokenRaw,
];
const GENERIC_DIMENSIONS: &[DetectionDimension] = &[
    DetectionDimension::TokenNormalized,
    DetectionDimension::TokenRaw,
    DetectionDimension::Line,
];

/// The full rule registry. Every rule ships enabled.
pub static RULES: &[SuppressionRule] = &[
    SuppressionRule {
        id: RuleId::AstSetterReturningSelf,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Ast],
        description: "builder setter: field assignment or simple mutation followed by `self`",
    },
    SuppressionRule {
        id: RuleId::AstForwardingAccessor,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Ast],
        description: "single method call forwarding simple values",
    },
    SuppressionRule {
        id: RuleId::AstBooleanProjection,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Ast],
        description: "bare boolean combination of simple projections",
    },
    SuppressionRule {
        id: RuleId::AstComparatorAdapter,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Ast],
        description: "closure of `key(a).cmp(&key(b))` shape",
    },
    SuppressionRule {
        id: RuleId::SubNoStructure,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::SubAst],
        description: "sub-unit without bindings, control flow, calls, or arithmetic",
    },
    SuppressionRule {
        id: RuleId::SubTrivialPredicate,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::SubAst],
        description: "comparison or boolean of simple values",
    },
    SuppressionRule {
        id: RuleId::SubEmptyDefaultReturn,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::SubAst],
        description: "guard returning an empty or default construction",
    },
    SuppressionRule {
        id: RuleId::SubMessageOnlyMacro,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::SubAst],
        description: "branch that only writes a message",
    },
    SuppressionRule {
        id: RuleId::SubValuePlumbing,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::SubAst],
        description: "call or method chain that only shuttles simple values",
    },
    SuppressionRule {
        id: RuleId::SubCoveredByChain,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::SubAst],
        description: "if-branch whose owning if-chain grouped as a whole",
    },
    SuppressionRule {
        id: RuleId::TokenImportScaffold,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: TOKEN_DIMENSIONS,
        description: "token window over import or module scaffolding",
    },
    SuppressionRule {
        id: RuleId::TokenChainTail,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: TOKEN_DIMENSIONS,
        description: "token window dominated by detached method-chain tails",
    },
    SuppressionRule {
        id: RuleId::TokenSignaturePrefix,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: TOKEN_DIMENSIONS,
        description: "token window cut from a doc-led signature prefix",
    },
    SuppressionRule {
        id: RuleId::TokenDeclarationScaffold,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: TOKEN_DIMENSIONS,
        description: "token window over type-declaration scaffolding",
    },
    SuppressionRule {
        id: RuleId::TokenMatchTablePrefix,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: TOKEN_DIMENSIONS,
        description: "token window stopping mid-way through a match-arm table",
    },
    SuppressionRule {
        id: RuleId::TokenLowSignal,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: TOKEN_DIMENSIONS,
        description: "token window without enough meaningful or unique content",
    },
    SuppressionRule {
        id: RuleId::LineImportScaffold,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Line],
        description: "line window over import or module scaffolding",
    },
    SuppressionRule {
        id: RuleId::LineChainTail,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Line],
        description: "line window dominated by detached method-chain tails",
    },
    SuppressionRule {
        id: RuleId::LineDeclarationSignaturePrefix,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Line],
        description: "line window over the opening rows of a fn declaration",
    },
    SuppressionRule {
        id: RuleId::LineLowSignal,
        level: RuleLevel::Unit,
        action: RuleAction::Suppress,
        dimensions: &[DetectionDimension::Line],
        description: "line window without enough meaningful or unique content",
    },
    SuppressionRule {
        id: RuleId::LineDeclarationStanza,
        level: RuleLevel::Unit,
        action: RuleAction::Admit,
        dimensions: &[DetectionDimension::Line],
        description: "uniform doc/attr/field stanza windows admitted across blank-separated declaration blocks",
    },
    SuppressionRule {
        id: RuleId::LineBuilderChainRun,
        level: RuleLevel::Unit,
        action: RuleAction::Admit,
        dimensions: &[DetectionDimension::Line],
        description: "windows made entirely of complete single-line builder steps",
    },
    SuppressionRule {
        id: RuleId::GroupCoveredByAst,
        level: RuleLevel::Group,
        action: RuleAction::Suppress,
        dimensions: GENERIC_DIMENSIONS,
        description: "token or line group fully covered by one AST or sub-AST group",
    },
    SuppressionRule {
        id: RuleId::GroupOverlapContained,
        level: RuleLevel::Group,
        action: RuleAction::Suppress,
        dimensions: GENERIC_DIMENSIONS,
        description: "window group contained within a wider same-dimension group",
    },
];

impl RuleId {
    /// The dotted string id used in config, CLI flags, stats, and reports.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AstSetterReturningSelf => "ast.setter-returning-self",
            Self::AstForwardingAccessor => "ast.forwarding-accessor",
            Self::AstBooleanProjection => "ast.boolean-projection",
            Self::AstComparatorAdapter => "ast.comparator-adapter",
            Self::SubNoStructure => "sub.no-structure",
            Self::SubTrivialPredicate => "sub.trivial-predicate",
            Self::SubEmptyDefaultReturn => "sub.empty-default-return",
            Self::SubMessageOnlyMacro => "sub.message-only-macro",
            Self::SubValuePlumbing => "sub.value-plumbing",
            Self::SubCoveredByChain => "sub.covered-by-chain",
            Self::TokenImportScaffold => "token.import-scaffold",
            Self::TokenChainTail => "token.chain-tail",
            Self::TokenSignaturePrefix => "token.signature-prefix",
            Self::TokenDeclarationScaffold => "token.declaration-scaffold",
            Self::TokenMatchTablePrefix => "token.match-table-prefix",
            Self::TokenLowSignal => "token.low-signal",
            Self::LineImportScaffold => "line.import-scaffold",
            Self::LineChainTail => "line.chain-tail",
            Self::LineDeclarationSignaturePrefix => "line.declaration-signature-prefix",
            Self::LineLowSignal => "line.low-signal",
            Self::LineDeclarationStanza => "line.declaration-stanza",
            Self::LineBuilderChainRun => "line.builder-chain-run",
            Self::GroupCoveredByAst => "group.covered-by-ast",
            Self::GroupOverlapContained => "group.overlap-contained",
        }
    }

    /// Parse a dotted rule id back to its identity.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::all().iter().copied().find(|id| id.as_str() == value)
    }

    /// Every rule id, in registry order.
    #[must_use]
    pub fn all() -> &'static [Self] {
        static ALL: std::sync::OnceLock<Vec<RuleId>> = std::sync::OnceLock::new();
        ALL.get_or_init(|| RULES.iter().map(|rule| rule.id).collect())
    }
}

/// The resolved active rule set: registry defaults plus config/CLI toggles.
#[derive(Debug, Clone, Default)]
pub struct SuppressionPolicy {
    disabled: BTreeSet<RuleId>,
}

impl SuppressionPolicy {
    /// Resolve a policy from disable/enable id lists.
    ///
    /// Enable wins over disable so CLI `--enable-rule` can override a config
    /// `[suppress] disable` entry. Unknown ids produce warnings, never errors.
    #[must_use]
    pub fn resolve(disable: &[String], enable: &[String]) -> (Self, Vec<String>) {
        let mut policy = Self::default();
        let mut warnings = Vec::new();
        let toggles = disable
            .iter()
            .map(|value| (value, true))
            .chain(enable.iter().map(|value| (value, false)));
        for (value, disabling) in toggles {
            let Some(id) = RuleId::parse(value) else {
                warnings.push(format!("unknown suppression rule id: {value}"));
                continue;
            };
            if disabling {
                policy.disabled.insert(id);
            } else {
                policy.disabled.remove(&id);
            }
        }
        (policy, warnings)
    }

    /// Apply additional disable/enable toggles on top of this policy.
    pub fn apply_toggles(&mut self, disable: &[String], enable: &[String]) -> Vec<String> {
        let (overlay, warnings) = Self::resolve(disable, enable);
        self.disabled.extend(overlay.disabled);
        for value in enable {
            if let Some(id) = RuleId::parse(value) {
                self.disabled.remove(&id);
            }
        }
        warnings
    }

    /// Whether a rule is active.
    #[must_use]
    pub fn is_enabled(&self, id: RuleId) -> bool {
        !self.disabled.contains(&id)
    }

    /// `Some(id)` iff the rule is active; classifiers compose with this.
    #[must_use]
    pub fn allow(&self, id: RuleId) -> Option<RuleId> {
        self.is_enabled(id).then_some(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rule_id_has_exactly_one_registry_row() {
        for id in RuleId::all() {
            assert_eq!(
                RULES.iter().filter(|rule| rule.id == *id).count(),
                1,
                "{} must have one row",
                id.as_str()
            );
        }
        assert_eq!(RULES.len(), RuleId::all().len());
    }

    #[test]
    fn rule_id_strings_are_unique_and_round_trip() {
        let mut seen = BTreeSet::new();
        for id in RuleId::all() {
            assert!(seen.insert(id.as_str()), "duplicate id {}", id.as_str());
            assert_eq!(RuleId::parse(id.as_str()), Some(*id));
        }
        assert_eq!(RuleId::parse("ast.unknown-rule"), None);
    }

    #[test]
    fn admit_rules_are_exactly_the_line_carve_outs() {
        let admits: Vec<RuleId> = RULES
            .iter()
            .filter(|rule| rule.action == RuleAction::Admit)
            .map(|rule| rule.id)
            .collect();
        assert_eq!(
            admits,
            vec![RuleId::LineDeclarationStanza, RuleId::LineBuilderChainRun]
        );
    }

    #[test]
    fn policy_resolution_applies_disable_then_enable_with_warnings() {
        let (policy, warnings) = SuppressionPolicy::resolve(
            &["line.chain-tail".to_string(), "no.such-rule".to_string()],
            &["line.chain-tail".to_string()],
        );
        assert!(policy.is_enabled(RuleId::LineChainTail));
        assert_eq!(warnings, vec!["unknown suppression rule id: no.such-rule"]);

        let (policy, warnings) =
            SuppressionPolicy::resolve(&["sub.value-plumbing".to_string()], &[]);
        assert!(warnings.is_empty());
        assert!(!policy.is_enabled(RuleId::SubValuePlumbing));
        assert_eq!(policy.allow(RuleId::SubValuePlumbing), None);
        assert_eq!(
            policy.allow(RuleId::SubNoStructure),
            Some(RuleId::SubNoStructure)
        );
    }

    #[test]
    fn toggles_layer_on_top_of_an_existing_policy() {
        let (mut policy, _) = SuppressionPolicy::resolve(&["line.chain-tail".to_string()], &[]);
        let warnings = policy.apply_toggles(
            &["token.low-signal".to_string()],
            &["line.chain-tail".to_string()],
        );
        assert!(warnings.is_empty());
        assert!(policy.is_enabled(RuleId::LineChainTail));
        assert!(!policy.is_enabled(RuleId::TokenLowSignal));
    }
}
