use crate::code_unit::CodeUnitKind;
use crate::node::{self, BinOpKind, NodeKind, NormalizedNode};

/// A sub-unit extracted from a normalized function body.
pub struct SubUnit {
    pub kind: CodeUnitKind,
    pub node: NormalizedNode,
    pub node_count: usize,
    pub description: String,
    /// Suppression rule that tagged this sub-unit as a low-signal candidate.
    pub suppressed: Option<crate::suppression::RuleId>,
    /// For if-branch sub-units, the fingerprint of the owning if-chain unit.
    pub parent_chain: Option<crate::fingerprint::Fingerprint>,
}

/// Bodies at or above this node count are never tagged as trivial shapes: a
/// large boolean projection or plumbing chain is worth seeing even when its
/// shape matches a trivial rule, while real setters and accessors sit well
/// under the cap.
pub const TRIVIAL_BODY_MAX_NODES: usize = 24;

/// Extract candidate sub-units from a normalized AST node.
///
/// Walks the tree recursively and extracts natural compound structures
/// (if branches, match arm bodies, loop bodies, closure bodies).
/// Each sub-tree is re-indexed to canonical placeholder form.
/// Only sub-trees meeting `min_node_count` are returned.
#[must_use]
pub fn extract_sub_units(node: &NormalizedNode, min_node_count: usize) -> Vec<SubUnit> {
    let mut results = Vec::new();
    extract_recursive(node, min_node_count, &mut results);
    results
}

/// Classify an extracted sub-unit against the suppression rules.
///
/// Direct extraction still returns every natural sub-tree that meets the node
/// threshold. The analysis pipeline tags each sub-unit with the first
/// matching enabled rule so tiny placeholders, projections, and simple
/// predicates do not become standalone visible findings; `None` means the
/// sub-unit is a fully visible candidate.
#[must_use]
pub fn classify_sub_unit(
    node: &NormalizedNode,
    policy: &crate::suppression::SuppressionPolicy,
) -> Option<crate::suppression::RuleId> {
    let node = peel_transparent_single_child(node);
    if !has_reportable_structure(node) {
        return policy.allow(crate::suppression::RuleId::SubNoStructure);
    }
    low_information_rule(node).and_then(|rule| policy.allow(rule))
}

fn extract_recursive(node: &NormalizedNode, min_node_count: usize, results: &mut Vec<SubUnit>) {
    // Block -> statement sequence: runs of consecutive `if` statements form
    // one coherent chain unit (for example option-to-field setter clusters),
    // and the chain replaces the per-branch units of its members.
    if matches!(node.kind, NodeKind::Block) && extract_if_chains(node, min_node_count, results) {
        return;
    }

    match &node.kind {
        // If -> [condition, then_branch, else_or_None]
        NodeKind::If => add_if_branches(node, min_node_count, results, None),
        // Match -> [expr, arm0, arm1, ...]
        // Each arm is MatchArm -> [pattern, guard_or_None, body]
        NodeKind::Match => {
            for (i, arm) in node.children.iter().skip(1).enumerate() {
                if let Some(body) = arm.children.get(2) {
                    let desc = format!("match arm {}", i + 1);
                    try_add(body, CodeUnitKind::MatchArm, &desc, min_node_count, results);
                }
            }
        }
        // Loop -> [body]
        NodeKind::Loop => {
            try_add_child(
                node,
                0,
                CodeUnitKind::LoopBody,
                "loop body",
                min_node_count,
                results,
            );
        }
        // While -> [condition, body]
        NodeKind::While => {
            try_add_child(
                node,
                1,
                CodeUnitKind::LoopBody,
                "while body",
                min_node_count,
                results,
            );
        }
        // ForLoop -> [pat, iter, body]
        NodeKind::ForLoop => {
            try_add_child(
                node,
                2,
                CodeUnitKind::LoopBody,
                "for body",
                min_node_count,
                results,
            );
        }
        // Closure -> [body, param0, ...]
        NodeKind::Closure => {
            try_add_child(
                node,
                0,
                CodeUnitKind::Block,
                "closure body",
                min_node_count,
                results,
            );
        }
        _ => {}
    }

    // Always recurse into all children
    for child in &node.children {
        extract_recursive(child, min_node_count, results);
    }
}

/// Emit the then/else branch units of an `if` node, linked to the owning
/// chain when the `if` belongs to one.
fn add_if_branches(
    node: &NormalizedNode,
    min_node_count: usize,
    results: &mut Vec<SubUnit>,
    parent_chain: Option<crate::fingerprint::Fingerprint>,
) {
    let mut add = |branch: &NormalizedNode, label: &str| {
        if try_add(
            branch,
            CodeUnitKind::IfBranch,
            label,
            min_node_count,
            results,
        ) {
            results.last_mut().expect("pushed").parent_chain = parent_chain;
        }
    };
    if let Some(then_branch) = node.children.get(1) {
        add(then_branch, "if-then branch");
    }
    if let Some(else_br) = node.children.get(2)
        && !else_br.is_none()
    {
        add(else_br, "if-else branch");
    }
}

/// Extract if-chain units from a block's statement children.
///
/// Returns false when the block has no chains, in which case the caller
/// falls through to ordinary extraction. When chains exist, the chain units
/// are added, per-branch units of chained statements are emitted linked to
/// their owning chain via `parent_chain`, and the children are recursed here
/// instead.
fn extract_if_chains(
    node: &NormalizedNode,
    min_node_count: usize,
    results: &mut Vec<SubUnit>,
) -> bool {
    let chain_members = if_chain_members(&node.children);
    if chain_members.is_empty() {
        return false;
    }
    let mut owning_chain: std::collections::HashMap<usize, crate::fingerprint::Fingerprint> =
        std::collections::HashMap::new();
    for run in consecutive_runs(&chain_members) {
        let chain = NormalizedNode::with_children(
            NodeKind::Block,
            run.iter()
                .map(|&idx| statement_if(&node.children[idx]).clone())
                .collect(),
        );
        let description = format!("if chain ({} branches)", run.len());
        if try_add(
            &chain,
            CodeUnitKind::IfChain,
            &description,
            min_node_count,
            results,
        ) {
            let chain_fp =
                crate::fingerprint::Fingerprint::from_node(&results.last().expect("pushed").node);
            for &idx in run {
                owning_chain.insert(idx, chain_fp);
            }
        }
    }
    for (idx, child) in node.children.iter().enumerate() {
        if chain_members.contains(&idx) {
            let chained_if = statement_if(child);
            if let Some(&chain_fp) = owning_chain.get(&idx) {
                // Branch units stay extracted but carry their owning chain so
                // the pipeline can treat them as covered while the chain
                // groups as a whole.
                add_if_branches(chained_if, min_node_count, results, Some(chain_fp));
            }
            for nested in &chained_if.children {
                extract_recursive(nested, min_node_count, results);
            }
        } else {
            extract_recursive(child, min_node_count, results);
        }
    }
    true
}

/// Return the `if` expression of a statement child, peeling a `Semi` wrapper.
fn statement_if(node: &NormalizedNode) -> &NormalizedNode {
    if matches!(node.kind, NodeKind::Semi)
        && node.children.len() == 1
        && matches!(node.children[0].kind, NodeKind::If)
    {
        &node.children[0]
    } else {
        node
    }
}

/// Indices of block statements that belong to an if-chain (a run of two or
/// more consecutive `if` statements).
fn if_chain_members(children: &[NormalizedNode]) -> Vec<usize> {
    let mut members = Vec::new();
    let mut run_start = None;
    for (idx, child) in children.iter().enumerate() {
        if matches!(statement_if(child).kind, NodeKind::If) {
            run_start.get_or_insert(idx);
        } else if let Some(start) = run_start.take()
            && idx - start >= 2
        {
            members.extend(start..idx);
        }
    }
    if let Some(start) = run_start
        && children.len() - start >= 2
    {
        members.extend(start..children.len());
    }
    members
}

/// Split sorted indices into runs of consecutive values.
fn consecutive_runs(indices: &[usize]) -> Vec<&[usize]> {
    crate::runs::split_runs_by(indices, |prev, curr| *curr == *prev + 1)
}

/// Extract the child at `child_index` as a sub-unit, if present.
fn try_add_child(
    node: &NormalizedNode,
    child_index: usize,
    kind: CodeUnitKind,
    description: &str,
    min_node_count: usize,
    results: &mut Vec<SubUnit>,
) {
    if let Some(child) = node.children.get(child_index) {
        try_add(child, kind, description, min_node_count, results);
    }
}

fn try_add(
    node: &NormalizedNode,
    kind: CodeUnitKind,
    description: &str,
    min_node_count: usize,
    results: &mut Vec<SubUnit>,
) -> bool {
    let reindexed = node::reindex_placeholders(node);
    let node_count = node::count_nodes(&reindexed);
    if node_count < min_node_count {
        return false;
    }
    results.push(SubUnit {
        suppressed: None,
        parent_chain: None,
        kind,
        node: reindexed,
        node_count,
        description: description.to_string(),
    });
    true
}

fn has_reportable_structure(node: &NormalizedNode) -> bool {
    match &node.kind {
        NodeKind::LetBinding
        | NodeKind::Assign
        | NodeKind::Return
        | NodeKind::Break
        | NodeKind::Continue
        | NodeKind::Call
        | NodeKind::MethodCall
        | NodeKind::MacroCall { .. }
        | NodeKind::If
        | NodeKind::Match
        | NodeKind::Loop
        | NodeKind::While
        | NodeKind::ForLoop
        | NodeKind::Yield
        | NodeKind::Await
        | NodeKind::Try
        | NodeKind::StructInit => true,
        NodeKind::BinaryOp(op) => {
            is_reportable_binary_op(op) || node.children.iter().any(has_reportable_structure)
        }
        NodeKind::UnaryOp(_)
        | NodeKind::Block
        | NodeKind::Paren
        | NodeKind::Semi
        | NodeKind::Tuple
        | NodeKind::Array
        | NodeKind::Set
        | NodeKind::Repeat
        | NodeKind::Range
        | NodeKind::Reference { .. }
        | NodeKind::Cast
        | NodeKind::FieldAccess
        | NodeKind::Index
        | NodeKind::Path
        | NodeKind::Closure => node.children.iter().any(has_reportable_structure),
        _ => false,
    }
}

const fn is_reportable_binary_op(op: &BinOpKind) -> bool {
    !matches!(
        op,
        BinOpKind::Eq
            | BinOpKind::Lt
            | BinOpKind::Le
            | BinOpKind::Ne
            | BinOpKind::Ge
            | BinOpKind::Gt
            | BinOpKind::And
            | BinOpKind::Or
            | BinOpKind::In
            | BinOpKind::NotIn
            | BinOpKind::Is
            | BinOpKind::IsNot
    )
}

/// The low-information shape of a node, attributed to its suppression rule.
fn low_information_rule(node: &NormalizedNode) -> Option<crate::suppression::RuleId> {
    use crate::suppression::RuleId;
    match &node.kind {
        NodeKind::BinaryOp(op) if !is_reportable_binary_op(op) => {
            children_are_simple_values_or_projections(node).then_some(RuleId::SubTrivialPredicate)
        }
        // A guard that bails out with an empty or default construction
        // (`return Vec::new()`, `return Config::default()`) carries no
        // structure of its own, and a dispatch return that only forwards
        // plumbing arguments through one call (`return normalize_x(a, b, c)`)
        // repeats wherever a dispatch table repeats; returns that wrap or
        // compute a value (`return Some(x)`, `return f(a + b, c)`) stay
        // reportable.
        NodeKind::Return => {
            if returns_empty_default(node) {
                Some(RuleId::SubEmptyDefaultReturn)
            } else if returns_plumbing_dispatch(node) {
                Some(RuleId::SubValuePlumbing)
            } else {
                None
            }
        }
        // A branch that only emits a message (`writeln!(writer, "...")?`)
        // repeats wherever something is printed, not where logic repeats.
        NodeKind::MacroCall { name } => (is_message_only_macro(name)
            && children_are_simple_values_or_projections(node))
        .then_some(RuleId::SubMessageOnlyMacro),
        // Bare constructor dispatch and value plumbing: calls whose
        // arguments only shuttle simple values through other plain calls
        // (`Box::new(RustAnalyzer::new())`, delegating match arms, visitor
        // forwarding bodies) only matter as part of a larger unit. Closures
        // keep a chain reportable: a pipeline with callback logic is a real
        // refactorable shape.
        NodeKind::Call | NodeKind::MethodCall => node
            .children
            .iter()
            .all(is_value_plumbing_expr)
            .then_some(RuleId::SubValuePlumbing),
        NodeKind::Block | NodeKind::Paren | NodeKind::Semi | NodeKind::Try
            if node.children.len() == 1 =>
        {
            low_information_rule(&node.children[0])
        }
        _ => None,
    }
}

fn children_are_simple_values_or_projections(node: &NormalizedNode) -> bool {
    node.children.iter().all(is_simple_value_or_projection)
}

/// Return true when a `Return` node only produces an empty/default value.
fn returns_empty_default(node: &NormalizedNode) -> bool {
    node.children.iter().all(|child| match &child.kind {
        NodeKind::Literal(_) => true,
        // `Vec::new()` / `MatchedGroups::default()`: a path called with no
        // arguments. `Some(value)` keeps its argument and stays reportable.
        NodeKind::Call => {
            child.children.len() == 1 && is_simple_value_or_projection(&child.children[0])
        }
        // `return rel.to_string_lossy()`: forwarding a projection.
        NodeKind::MethodCall => children_are_simple_values_or_projections(child),
        _ => false,
    })
}

/// Return true for a dispatch-style return: exactly one child of kind
/// `Call` with a callee and at least two arguments, all value plumbing
/// (`return normalize_if(node, source, mapping, ctx)`). Two-child calls
/// (`return Some(x)`, `return NormalizedNode::leaf(kind)`) stay reportable.
fn returns_plumbing_dispatch(node: &NormalizedNode) -> bool {
    let [child] = node.children.as_slice() else {
        return false;
    };
    matches!(child.kind, NodeKind::Call)
        && child.children.len() >= 3
        && child.children.iter().all(is_value_plumbing_expr)
}

/// Macros whose only effect is writing a message to an output stream.
const MESSAGE_ONLY_MACROS: &[&str] =
    &["print", "println", "eprint", "eprintln", "write", "writeln"];

fn is_message_only_macro(name: &str) -> bool {
    MESSAGE_ONLY_MACROS.contains(&name)
}

/// Return true for expressions that only shuttle values around: simple
/// values/projections, and calls or value macros built from them. Closures
/// are not plumbing; callback logic makes a chain reportable.
fn is_value_plumbing_expr(node: &NormalizedNode) -> bool {
    if is_simple_value_or_projection(node) {
        return true;
    }
    match &node.kind {
        NodeKind::Call
        | NodeKind::MethodCall
        | NodeKind::MacroCall { .. }
        | NodeKind::Reference { .. }
        | NodeKind::Paren => node.children.iter().all(is_value_plumbing_expr),
        _ => false,
    }
}

fn is_simple_value_or_projection(node: &NormalizedNode) -> bool {
    match &node.kind {
        NodeKind::Literal(_)
        | NodeKind::Placeholder(_, _)
        | NodeKind::PatPlaceholder(_, _)
        | NodeKind::TypePlaceholder(_, _)
        | NodeKind::PatWild
        | NodeKind::PatLiteral
        | NodeKind::TypeInfer
        | NodeKind::TypeUnit
        | NodeKind::None
        | NodeKind::Opaque
        | NodeKind::Token(_) => true,
        NodeKind::Path
        | NodeKind::FieldAccess
        | NodeKind::Index
        | NodeKind::Reference { .. }
        | NodeKind::Cast
        | NodeKind::UnaryOp(_)
        | NodeKind::Paren
        | NodeKind::Tuple
        | NodeKind::Array
        | NodeKind::Set
        | NodeKind::Range
        | NodeKind::TypeReference { .. }
        | NodeKind::TypeTuple
        | NodeKind::TypeSlice
        | NodeKind::TypeArray
        | NodeKind::TypePath
        | NodeKind::TypeImplTrait
        | NodeKind::PatTuple
        | NodeKind::PatStruct
        | NodeKind::PatOr
        | NodeKind::PatReference { .. }
        | NodeKind::PatSlice
        | NodeKind::PatRest
        | NodeKind::PatRange => node.children.iter().all(is_simple_value_or_projection),
        NodeKind::Block | NodeKind::Semi if node.children.len() == 1 => {
            is_simple_value_or_projection(&node.children[0])
        }
        _ => false,
    }
}

fn peel_transparent_single_child(mut node: &NormalizedNode) -> &NormalizedNode {
    while matches!(
        node.kind,
        NodeKind::Block | NodeKind::Paren | NodeKind::Semi
    ) && node.children.len() == 1
    {
        node = &node.children[0];
    }
    node
}

/// Classify a top-level function or method body against the suppression rules.
///
/// Whole-body trivial shapes — builder setters, accessor forwarding, and
/// bare boolean projection predicates — repeat wherever the language forces
/// the pattern, so they are tagged rather than reported by default. Bodies
/// with bindings, control flow, arithmetic, closures, or multi-statement
/// behavior stay visible (`None`), as do constructor wrappers that bind
/// constants.
#[must_use]
pub fn classify_top_level_body(
    body: &NormalizedNode,
    policy: &crate::suppression::SuppressionPolicy,
) -> Option<crate::suppression::RuleId> {
    use crate::suppression::RuleId;
    let body = peel_transparent_single_child(body);
    if node::count_nodes(body) >= TRIVIAL_BODY_MAX_NODES {
        return None;
    }
    if is_setter_returning_self(body) {
        return policy.allow(RuleId::AstSetterReturningSelf);
    }
    if is_forwarding_accessor_body(body) {
        return policy.allow(RuleId::AstForwardingAccessor);
    }
    if is_trivial_boolean_projection(body) {
        return policy.allow(RuleId::AstBooleanProjection);
    }
    None
}

/// Classify a standalone closure body against the suppression rules.
///
/// Applies the top-level rules plus the comparator-adapter shape
/// (`key(a).cmp(&key(b))`), which closures repeat at every sort site.
#[must_use]
pub fn classify_closure_body(
    body: &NormalizedNode,
    policy: &crate::suppression::SuppressionPolicy,
) -> Option<crate::suppression::RuleId> {
    classify_top_level_body(body, policy).or_else(|| {
        if is_comparator_adapter(peel_transparent_single_child(body)) {
            policy.allow(crate::suppression::RuleId::AstComparatorAdapter)
        } else {
            None
        }
    })
}

/// `self.field = value; self` builder-setter bodies.
fn is_setter_returning_self(node: &NormalizedNode) -> bool {
    if !matches!(node.kind, NodeKind::Block) || node.children.len() != 2 {
        return false;
    }
    if !is_simple_value_or_projection(&node.children[1]) {
        return false;
    }
    let mutation = peel_transparent_single_child(&node.children[0]);
    // Assignment setters (`self.f = v; self`) and simple method mutations
    // (`self.f.push(v); self`) are one rule: the language forces the shape
    // either way. Closure-bearing mutations stay reportable via the
    // value-plumbing test.
    match mutation.kind {
        NodeKind::Assign | NodeKind::MethodCall => {
            mutation.children.iter().all(is_value_plumbing_expr)
        }
        _ => false,
    }
}

/// A single method call forwarding simple values (`self.a(self.b)`).
fn is_forwarding_accessor_body(node: &NormalizedNode) -> bool {
    matches!(node.kind, NodeKind::MethodCall) && children_are_simple_values_or_projections(node)
}

/// A bare boolean combination of simple projections
/// (`ch == '_' || ch.is_ascii_alphanumeric()`).
fn is_trivial_boolean_projection(node: &NormalizedNode) -> bool {
    match &node.kind {
        NodeKind::BinaryOp(op) if !is_reportable_binary_op(op) => {
            node.children.iter().all(|child| {
                is_simple_value_or_projection(child)
                    || is_forwarding_accessor_body(child)
                    || is_trivial_boolean_projection(child)
            })
        }
        _ => false,
    }
}

/// `key(a).cmp(&key(b))`-shaped comparator adapters.
fn is_comparator_adapter(node: &NormalizedNode) -> bool {
    matches!(node.kind, NodeKind::MethodCall) && node.children.iter().all(is_value_plumbing_expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{LiteralKind, PlaceholderKind};
    use crate::suppression::{RuleId, SuppressionPolicy};

    fn policy() -> SuppressionPolicy {
        SuppressionPolicy::default()
    }

    // jscpd:ignore-start

    fn variable(index: usize) -> NormalizedNode {
        NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, index))
    }

    fn literal_int() -> NormalizedNode {
        NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Int))
    }

    fn block(children: Vec<NormalizedNode>) -> NormalizedNode {
        NormalizedNode::with_children(NodeKind::Block, children)
    }

    fn binary(op: BinOpKind, left: NormalizedNode, right: NormalizedNode) -> NormalizedNode {
        NormalizedNode::with_children(NodeKind::BinaryOp(op), vec![left, right])
    }

    // jscpd:ignore-end

    #[test]
    fn reportable_sub_unit_rejects_placeholder_only_blocks() {
        assert_eq!(
            classify_sub_unit(&block(vec![variable(0)]), &policy()),
            Some(RuleId::SubNoStructure)
        );
    }

    #[test]
    fn reportable_sub_unit_rejects_simple_predicates() {
        let field_access =
            NormalizedNode::with_children(NodeKind::FieldAccess, vec![variable(0), variable(1)]);
        let predicate = binary(BinOpKind::Eq, field_access, variable(2));

        // Bare comparisons of simple values carry no reportable structure at
        // all, so the no-structure rule fires before the trivial-predicate
        // arm (which only applies to structured comparisons).
        assert_eq!(
            classify_sub_unit(&predicate, &policy()),
            Some(RuleId::SubNoStructure)
        );
        assert_eq!(
            classify_sub_unit(&block(vec![predicate]), &policy()),
            Some(RuleId::SubNoStructure)
        );
    }

    #[test]
    fn reportable_sub_unit_keeps_arithmetic_branch_body() {
        let arithmetic = block(vec![binary(BinOpKind::Add, variable(0), literal_int())]);

        assert_eq!(classify_sub_unit(&arithmetic, &policy()), None);
    }

    #[test]
    fn reportable_sub_unit_keeps_binding_branch_body() {
        let binding = NormalizedNode::with_children(
            NodeKind::LetBinding,
            vec![
                variable(0),
                NormalizedNode::none(),
                literal_int(),
                NormalizedNode::none(),
            ],
        );

        assert_eq!(classify_sub_unit(&block(vec![binding]), &policy()), None);
    }

    #[test]
    fn direct_extraction_still_respects_only_node_threshold() {
        let body = NormalizedNode::with_children(
            NodeKind::If,
            vec![
                variable(0),
                block(vec![variable(1)]),
                NormalizedNode::none(),
            ],
        );

        let sub_units = extract_sub_units(&body, 1);

        assert_eq!(sub_units.len(), 1);
        assert_eq!(
            classify_sub_unit(&sub_units[0].node, &policy()),
            Some(RuleId::SubNoStructure)
        );
    }

    fn call(children: Vec<NormalizedNode>) -> NormalizedNode {
        NormalizedNode::with_children(NodeKind::Call, children)
    }

    #[test]
    fn reportable_sub_unit_rejects_forwarding_call_bodies() {
        // `normalize_pat(&p.inner, ctx)`-style delegation: a call whose
        // arguments are all simple projections or placeholders.
        let field_access =
            NormalizedNode::with_children(NodeKind::FieldAccess, vec![variable(0), variable(1)]);
        let reference = NormalizedNode::with_children(
            NodeKind::Reference { mutable: false },
            vec![field_access],
        );
        let forwarding = call(vec![
            NormalizedNode::leaf(NodeKind::Path),
            reference,
            variable(2),
        ]);

        assert_eq!(
            classify_sub_unit(&forwarding, &policy()),
            Some(RuleId::SubValuePlumbing)
        );
        assert_eq!(
            classify_sub_unit(&block(vec![forwarding]), &policy()),
            Some(RuleId::SubValuePlumbing)
        );
    }

    #[test]
    fn reportable_sub_unit_keeps_structured_constructor_bodies() {
        // `NormalizedNode::with_children(kind, items.iter().map(...).collect())`
        // carries a nested closure pipeline and stays reportable.
        let closure = NormalizedNode::with_children(NodeKind::Closure, vec![variable(0)]);
        let map_call =
            NormalizedNode::with_children(NodeKind::MethodCall, vec![variable(1), closure]);
        let collect_call = NormalizedNode::with_children(NodeKind::MethodCall, vec![map_call]);
        let constructor = call(vec![
            NormalizedNode::leaf(NodeKind::Path),
            NormalizedNode::leaf(NodeKind::Path),
            collect_call,
        ]);

        assert_eq!(classify_sub_unit(&constructor, &policy()), None);
    }

    fn method_call(children: Vec<NormalizedNode>) -> NormalizedNode {
        NormalizedNode::with_children(NodeKind::MethodCall, children)
    }

    fn semi(child: NormalizedNode) -> NormalizedNode {
        NormalizedNode::with_children(NodeKind::Semi, vec![child])
    }

    #[test]
    fn guard_returns_of_empty_defaults_are_not_reported() {
        // `if shorted { return Vec::new(); }`-style bail-out guards.
        let empty_default = call(vec![NormalizedNode::leaf(NodeKind::Path)]);
        let guard = block(vec![semi(NormalizedNode::with_children(
            NodeKind::Return,
            vec![empty_default],
        ))]);

        assert_eq!(
            classify_sub_unit(&guard, &policy()),
            Some(RuleId::SubEmptyDefaultReturn)
        );
    }

    #[test]
    fn guard_returns_wrapping_values_stay_reported() {
        // `if !text.is_empty() { return Some(text); }` stays a reportable
        // shape: the return carries a constructed value, not a bare default.
        let some_value = call(vec![NormalizedNode::leaf(NodeKind::Path), variable(0)]);
        let guard = block(vec![semi(NormalizedNode::with_children(
            NodeKind::Return,
            vec![some_value],
        ))]);

        assert_eq!(classify_sub_unit(&guard, &policy()), None);
    }

    #[test]
    fn computed_returns_stay_reported() {
        let computed = NormalizedNode::with_children(
            NodeKind::Return,
            vec![binary(BinOpKind::Add, variable(0), variable(1))],
        );

        assert_eq!(
            classify_sub_unit(&block(vec![semi(computed)]), &policy()),
            None
        );
    }

    #[test]
    fn error_returns_with_payloads_stay_reported() {
        let err_value = call(vec![NormalizedNode::leaf(NodeKind::Path), variable(0)]);
        let return_err = NormalizedNode::with_children(NodeKind::Return, vec![err_value]);

        assert_eq!(
            classify_sub_unit(&block(vec![semi(return_err)]), &policy()),
            None
        );
    }

    #[test]
    fn plumbing_dispatch_return_is_tagged_value_plumbing() {
        // `return normalize_if(node, source, mapping, ctx)`: one call with a
        // callee and >= 2 plumbing arguments is dispatch, not logic.
        let dispatch = call(vec![
            NormalizedNode::leaf(NodeKind::Path),
            variable(0),
            variable(1),
            variable(2),
            variable(3),
        ]);
        let body = block(vec![semi(NormalizedNode::with_children(
            NodeKind::Return,
            vec![dispatch],
        ))]);

        assert_eq!(
            classify_sub_unit(&body, &policy()),
            Some(RuleId::SubValuePlumbing)
        );
    }

    #[test]
    fn return_some_value_stays_reportable() {
        // `return Some(x)`: a two-child call (callee + one argument) is a
        // wrapped value, never dispatch plumbing.
        let some_value = call(vec![NormalizedNode::leaf(NodeKind::Path), variable(0)]);
        let wrapped = NormalizedNode::with_children(NodeKind::Return, vec![some_value]);

        assert_eq!(classify_sub_unit(&wrapped, &policy()), None);
    }

    #[test]
    fn return_with_computed_argument_stays_reportable() {
        // `return f(a + b, c)`: a computed argument makes the call logic,
        // however many arguments it forwards.
        let computed_arg = binary(BinOpKind::Add, variable(0), variable(1));
        let dispatch = call(vec![
            NormalizedNode::leaf(NodeKind::Path),
            computed_arg,
            variable(2),
        ]);
        let body = NormalizedNode::with_children(NodeKind::Return, vec![dispatch]);

        assert_eq!(classify_sub_unit(&body, &policy()), None);
    }

    #[test]
    fn message_only_macro_branches_are_not_reported() {
        // `{ writeln!(writer, "No stale entries found.")?; }`
        let message = NormalizedNode::with_children(
            NodeKind::MacroCall {
                name: "writeln".to_string(),
            },
            vec![
                variable(0),
                NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Str)),
            ],
        );
        let branch = block(vec![semi(NormalizedNode::with_children(
            NodeKind::Try,
            vec![message],
        ))]);

        assert_eq!(
            classify_sub_unit(&branch, &policy()),
            Some(RuleId::SubMessageOnlyMacro)
        );
    }

    #[test]
    fn assertion_macro_branches_stay_reported() {
        let oracle = NormalizedNode::with_children(
            NodeKind::MacroCall {
                name: "assert_eq".to_string(),
            },
            vec![variable(0), variable(1)],
        );

        assert_eq!(
            classify_sub_unit(&block(vec![semi(oracle)]), &policy()),
            None
        );
    }

    #[test]
    fn panic_macro_branches_stay_reported() {
        let panic = NormalizedNode::with_children(
            NodeKind::MacroCall {
                name: "panic".to_string(),
            },
            vec![NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Str))],
        );

        assert_eq!(
            classify_sub_unit(&block(vec![semi(panic)]), &policy()),
            None
        );
    }

    #[test]
    fn constructor_dispatch_arms_are_not_reported() {
        // `Language::Rust => Box::new(RustAnalyzer::new())`
        let new_call = call(vec![NormalizedNode::leaf(NodeKind::Path)]);
        let dispatch = call(vec![NormalizedNode::leaf(NodeKind::Path), new_call]);

        assert_eq!(
            classify_sub_unit(&dispatch, &policy()),
            Some(RuleId::SubValuePlumbing)
        );
    }

    #[test]
    fn constructor_assembly_arms_are_not_reported() {
        // `NormalizedNode::with_children(KIND, vec![norm(a), norm(b)])`
        let assembly = call(vec![
            NormalizedNode::leaf(NodeKind::Path),
            NormalizedNode::leaf(NodeKind::Path),
            NormalizedNode::with_children(
                NodeKind::MacroCall {
                    name: "vec".to_string(),
                },
                vec![
                    call(vec![variable(0), variable(1)]),
                    call(vec![variable(0), variable(2)]),
                ],
            ),
        ]);

        assert_eq!(
            classify_sub_unit(&assembly, &policy()),
            Some(RuleId::SubValuePlumbing)
        );
    }

    #[test]
    fn collection_mutation_branches_are_not_reported() {
        // `{ files.push(path.to_path_buf()); }`
        let projection = method_call(vec![variable(0), variable(1)]);
        let push = method_call(vec![variable(2), variable(3), projection]);

        assert_eq!(
            classify_sub_unit(&block(vec![semi(push)]), &policy()),
            Some(RuleId::SubValuePlumbing)
        );
    }

    #[test]
    fn projection_only_method_predicates_are_not_reported() {
        // `node.children.iter().any(is_simple_value_or_projection)`
        let field =
            NormalizedNode::with_children(NodeKind::FieldAccess, vec![variable(0), variable(1)]);
        let iter = method_call(vec![field, variable(2)]);
        let any = method_call(vec![iter, variable(3), variable(4)]);

        assert_eq!(
            classify_sub_unit(&any, &policy()),
            Some(RuleId::SubValuePlumbing)
        );
    }

    #[test]
    fn value_plumbing_with_nested_closures_stays_reported() {
        let callback = NormalizedNode::with_children(
            NodeKind::Closure,
            vec![binary(BinOpKind::Add, variable(0), literal_int())],
        );
        let pipeline = method_call(vec![variable(1), variable(2), callback]);

        assert_eq!(classify_sub_unit(&block(vec![pipeline]), &policy()), None);
    }

    #[test]
    fn top_level_builder_setters_are_not_reportable() {
        // `self.kind_resolver = Some(resolver); self`
        let assign = NormalizedNode::with_children(
            NodeKind::Assign,
            vec![
                NormalizedNode::with_children(
                    NodeKind::FieldAccess,
                    vec![variable(0), variable(1)],
                ),
                call(vec![NormalizedNode::leaf(NodeKind::Path), variable(2)]),
            ],
        );
        let body = block(vec![semi(assign), variable(0)]);

        assert!(classify_top_level_body(&body, &policy()).is_some());
    }

    #[test]
    fn top_level_accessor_forwarding_is_not_reportable() {
        // `self.percent_of_total(self.exact_duplicate_lines)`
        let projection =
            NormalizedNode::with_children(NodeKind::FieldAccess, vec![variable(0), variable(1)]);
        let body = method_call(vec![variable(0), variable(2), projection]);

        assert!(classify_top_level_body(&block(vec![body]), &policy()).is_some());
    }

    #[test]
    fn top_level_boolean_projections_are_not_reportable() {
        // `ch == '_' || ch.is_ascii_alphanumeric()`
        let eq = binary(BinOpKind::Eq, variable(0), literal_int());
        let predicate_call = method_call(vec![variable(0), variable(1)]);
        let body = binary(BinOpKind::Or, eq, predicate_call);

        assert!(classify_top_level_body(&block(vec![body]), &policy()).is_some());
    }

    #[test]
    fn top_level_constant_binding_wrappers_stay_reportable() {
        // `fixture_path("cargo-dupes", name)`: a Call-rooted wrapper that
        // binds a constant is a deliberate named specialization.
        let body = call(vec![
            variable(0),
            NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Str)),
            variable(1),
        ]);

        assert_eq!(classify_top_level_body(&block(vec![body]), &policy()), None);
    }

    #[test]
    fn top_level_struct_constructors_stay_reportable() {
        // `Self { root }`
        let field =
            NormalizedNode::with_children(NodeKind::FieldValue, vec![variable(0), variable(0)]);
        let init = NormalizedNode::with_children(
            NodeKind::StructInit,
            vec![NormalizedNode::none(), field],
        );

        assert_eq!(classify_top_level_body(&block(vec![init]), &policy()), None);
    }

    #[test]
    fn closure_comparator_adapters_are_not_reportable() {
        // `|| group_start_key(a).cmp(&group_start_key(b))`
        let key_a = call(vec![variable(0), variable(1)]);
        let key_b = call(vec![variable(0), variable(2)]);
        let cmp = method_call(vec![
            key_a,
            variable(3),
            NormalizedNode::with_children(NodeKind::Reference { mutable: false }, vec![key_b]),
        ]);

        assert_eq!(
            classify_closure_body(&cmp, &policy()),
            Some(RuleId::AstComparatorAdapter)
        );
    }

    #[test]
    fn closures_with_callback_pipelines_stay_reportable() {
        // `path.segments.iter().map(|s| s.ident.to_string()).join("::")`:
        // the inner closure carries logic, so the chain stays reportable.
        let projection =
            NormalizedNode::with_children(NodeKind::FieldAccess, vec![variable(0), variable(1)]);
        let inner_closure = NormalizedNode::with_children(
            NodeKind::Closure,
            vec![method_call(vec![projection, variable(2)])],
        );
        let chain = method_call(vec![
            method_call(vec![variable(3), variable(4)]),
            variable(5),
            inner_closure,
        ]);

        assert_eq!(
            classify_closure_body(&block(vec![chain.clone()]), &policy()),
            None
        );
        assert_eq!(classify_sub_unit(&block(vec![chain]), &policy()), None);
    }

    fn setter_if(target: usize, value: usize) -> NormalizedNode {
        let assign = NormalizedNode::with_children(
            NodeKind::Assign,
            vec![variable(target), variable(value)],
        );
        NormalizedNode::with_children(
            NodeKind::If,
            vec![
                binary(BinOpKind::Gt, variable(value), literal_int()),
                block(vec![assign]),
                NormalizedNode::none(),
            ],
        )
    }

    #[test]
    fn consecutive_setter_ifs_extract_as_one_chain() {
        let body = block(vec![setter_if(0, 1), setter_if(2, 3), setter_if(4, 5)]);

        let sub_units = extract_sub_units(&body, 1);

        let chains: Vec<_> = sub_units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::IfChain)
            .collect();
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].description, "if chain (3 branches)");
        let chain_fp = crate::fingerprint::Fingerprint::from_node(&chains[0].node);
        let branches: Vec<_> = sub_units
            .iter()
            .filter(|unit| unit.kind == CodeUnitKind::IfBranch)
            .collect();
        assert_eq!(
            branches.len(),
            3,
            "chained branches stay extracted, linked to their owning chain"
        );
        for branch in branches {
            assert_eq!(branch.parent_chain, Some(chain_fp));
        }
        assert_eq!(classify_sub_unit(&chains[0].node, &policy()), None);
    }

    #[test]
    fn single_if_statement_still_extracts_branch_not_chain() {
        let body = block(vec![setter_if(0, 1)]);

        let sub_units = extract_sub_units(&body, 1);

        assert!(
            sub_units
                .iter()
                .any(|unit| unit.kind == CodeUnitKind::IfBranch)
        );
        assert!(
            sub_units
                .iter()
                .all(|unit| unit.kind != CodeUnitKind::IfChain)
        );
    }

    #[test]
    fn chain_members_still_extract_nested_structures() {
        let inner_loop = NormalizedNode::with_children(
            NodeKind::While,
            vec![
                binary(BinOpKind::Gt, variable(0), literal_int()),
                block(vec![binary(BinOpKind::Add, variable(1), literal_int())]),
            ],
        );
        let if_with_loop = NormalizedNode::with_children(
            NodeKind::If,
            vec![
                binary(BinOpKind::Gt, variable(0), literal_int()),
                block(vec![inner_loop]),
                NormalizedNode::none(),
            ],
        );
        let body = block(vec![if_with_loop, setter_if(2, 3)]);

        let sub_units = extract_sub_units(&body, 1);

        assert!(
            sub_units
                .iter()
                .any(|unit| unit.kind == CodeUnitKind::IfChain)
        );
        assert!(
            sub_units
                .iter()
                .any(|unit| unit.kind == CodeUnitKind::LoopBody),
            "structures nested inside chain branches are still extracted"
        );
    }
}
