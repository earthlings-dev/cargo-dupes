//! Table-driven [`NodeMapping`] from tree-sitter grammar node kinds to
//! `dupes-core` concepts — the extension point each language populates.

use std::collections::HashMap;
use std::collections::HashSet;

use dupes_core::node::BinOpKind;
use dupes_core::node::LiteralKind;
use dupes_core::node::NodeKind;
use dupes_core::node::UnOpKind;

/// Table-driven mapping from tree-sitter node kinds to dupes-core concepts.
///
/// Each language populates its own instance with the relevant node kind strings
/// from its tree-sitter grammar. The normalizer and extractor use this mapping
/// to convert tree-sitter CST nodes into `NormalizedNode` trees.
#[derive(Debug, Clone)]
pub struct NodeMapping {
  /// Node kinds that represent identifiers (variables, function names, etc.).
  pub identifier_kinds:   HashSet<&'static str>,
  /// Node kinds that represent literals, mapped to their `LiteralKind`.
  pub literal_kinds:      HashMap<&'static str, LiteralKind>,
  /// Node kinds for binary operators, mapped to `BinOpKind`.
  pub binary_op_map:      HashMap<&'static str, BinOpKind>,
  /// Node kinds for unary operators, mapped to `UnOpKind`.
  pub unary_op_map:       HashMap<&'static str, UnOpKind>,
  /// Node kinds to skip entirely (e.g., comments, decorators).
  pub skip_kinds:         HashSet<&'static str>,
  /// Node kinds to treat as opaque leaves (no recursive normalization).
  pub opaque_kinds:       HashSet<&'static str>,
  /// Node kinds representing block/suite constructs.
  pub block_kinds:        HashSet<&'static str>,
  /// Node kinds representing function/method calls.
  pub call_kinds:         HashSet<&'static str>,
  /// Node kinds representing return statements.
  pub return_kinds:       HashSet<&'static str>,
  /// Node kinds representing if/conditional constructs.
  pub if_kinds:           HashSet<&'static str>,
  /// Node kinds representing infinite loop constructs.
  pub loop_kinds:         HashSet<&'static str>,
  /// Node kinds representing for-loop constructs.
  pub for_kinds:          HashSet<&'static str>,
  /// Node kinds representing while-loop constructs.
  pub while_kinds:        HashSet<&'static str>,
  /// Node kinds representing match/switch constructs.
  pub match_kinds:        HashSet<&'static str>,
  /// Node kinds representing assignment statements.
  pub assignment_kinds:   HashSet<&'static str>,
  /// Node kinds representing function definitions.
  pub function_def_kinds: HashSet<&'static str>,
  /// Node kinds representing binary operator expressions (e.g., `"binary_operator"`,
  /// `"binary_expression"`, `"boolean_operator"`, `"comparison_operator"`).
  /// The operator text is looked up in `binary_op_map`.
  pub binary_op_kinds:    HashSet<&'static str>,
  /// Node kinds representing unary operator expressions (e.g., `"unary_operator"`,
  /// `"unary_expression"`, `"not_operator"`).
  /// The operator text is looked up in `unary_op_map`.
  pub unary_op_kinds:     HashSet<&'static str>,
  /// Node kinds representing match/case arm entries within a match statement.
  /// Used for fixed-position child extraction: `[pattern, guard_or_None, body]`.
  pub match_arm_kinds:    HashSet<&'static str>,
  /// Direct node-kind-to-`NodeKind` mappings. Named children are recursively
  /// normalized and attached as children. Zero-child nodes produce leaves.
  ///
  /// Use this for constructs that map directly to a `NodeKind` variant and
  /// whose children should be normalized generically (e.g., `break_statement` →
  /// `Break`, `await` → `Await [child]`, `tuple` → `Tuple [elem, ...]`).
  pub node_kinds:         HashMap<&'static str, NodeKind>,
}

macro_rules! set_builder {
    ($(
        $(#[$meta:meta])*
        $method:ident => $field:ident;
    )*) => {
        $(
            $(#[$meta])*
            #[must_use]
            pub fn $method(mut self, kinds: &[&'static str]) -> Self {
                extend_set(&mut self.$field, kinds);
                self
            }
        )*
    };
}

macro_rules! map_builder {
    ($(
        $(#[$meta:meta])*
        $method:ident($value:ty) => $field:ident;
    )*) => {
        $(
            $(#[$meta])*
            #[must_use]
            pub fn $method(mut self, mappings: &[(&'static str, $value)]) -> Self {
                extend_map(&mut self.$field, mappings);
                self
            }
        )*
    };
}

impl NodeMapping {
  /// Create an empty mapping. Use the builder methods to populate it.
  #[must_use]
  pub fn new() -> Self {
    Self {
      identifier_kinds:   HashSet::new(),
      literal_kinds:      HashMap::new(),
      binary_op_map:      HashMap::new(),
      unary_op_map:       HashMap::new(),
      skip_kinds:         HashSet::new(),
      opaque_kinds:       HashSet::new(),
      block_kinds:        HashSet::new(),
      call_kinds:         HashSet::new(),
      return_kinds:       HashSet::new(),
      if_kinds:           HashSet::new(),
      loop_kinds:         HashSet::new(),
      for_kinds:          HashSet::new(),
      while_kinds:        HashSet::new(),
      match_kinds:        HashSet::new(),
      assignment_kinds:   HashSet::new(),
      function_def_kinds: HashSet::new(),
      binary_op_kinds:    HashSet::new(),
      unary_op_kinds:     HashSet::new(),
      match_arm_kinds:    HashSet::new(),
      node_kinds:         HashMap::new(),
    }
  }

  set_builder! {
      /// Add identifier node kinds.
      identifiers => identifier_kinds;
      /// Add node kinds to skip entirely.
      skip => skip_kinds;
      /// Add node kinds to treat as opaque leaves.
      opaque => opaque_kinds;
      /// Add block/suite node kinds.
      blocks => block_kinds;
      /// Add call node kinds.
      calls => call_kinds;
      /// Add return statement node kinds.
      returns => return_kinds;
      /// Add if/conditional node kinds.
      ifs => if_kinds;
      /// Add infinite loop node kinds.
      loops => loop_kinds;
      /// Add for-loop node kinds.
      for_loops => for_kinds;
      /// Add while-loop node kinds.
      while_loops => while_kinds;
      /// Add match/switch node kinds.
      matches => match_kinds;
      /// Add assignment node kinds.
      assignments => assignment_kinds;
      /// Add function definition node kinds.
      function_defs => function_def_kinds;
      /// Add binary operator expression node kinds (e.g., `"binary_operator"`,
      /// `"binary_expression"`). These are the tree-sitter node kinds that contain
      /// a binary operation; the operator text is looked up in `binary_op_map`.
      binary_op_kinds => binary_op_kinds;
      /// Add unary operator expression node kinds (e.g., `"unary_operator"`,
      /// `"not_operator"`). These are the tree-sitter node kinds that contain
      /// a unary operation; the operator text is looked up in `unary_op_map`.
      unary_op_kinds => unary_op_kinds;
      /// Add match/case arm node kinds for fixed-position extraction.
      match_arms => match_arm_kinds;
  }

  map_builder! {
      /// Add literal node kinds with their `LiteralKind`.
      literals(LiteralKind) => literal_kinds;
      /// Add binary operator mappings (operator text → `BinOpKind`).
      binary_ops(BinOpKind) => binary_op_map;
      /// Add unary operator mappings (operator text → `UnOpKind`).
      unary_ops(UnOpKind) => unary_op_map;
      /// Add direct node-kind-to-`NodeKind` mappings.
      ///
      /// Named children are recursively normalized. Zero-child nodes produce leaves.
      /// Use this for constructs like `break_statement` → `Break`, `await` → `Await`.
      node_kinds(NodeKind) => node_kinds;
  }
}

fn extend_set(set: &mut HashSet<&'static str>, values: &[&'static str]) {
  set.extend(values.iter().copied());
}

fn extend_map<T: Clone>(map: &mut HashMap<&'static str, T>, values: &[(&'static str, T)]) {
  map.extend(values.iter().map(|(key, value)| (*key, value.clone())));
}

impl Default for NodeMapping {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_mapping() {
    let m = NodeMapping::new();
    assert!(m.identifier_kinds.is_empty());
    assert!(m.literal_kinds.is_empty());
    assert!(m.binary_op_map.is_empty());
  }

  // jscpd:ignore-start

  #[test]
  fn builder_api() {
    let m = NodeMapping::new()
      .identifiers(&["identifier", "name"])
      .literals(&[("integer", LiteralKind::Int), ("string", LiteralKind::Str)])
      .binary_ops(&[("+", BinOpKind::Add), ("-", BinOpKind::Sub)])
      .skip(&["comment"])
      .blocks(&["block"])
      .calls(&["call"])
      .returns(&["return_statement"])
      .ifs(&["if_statement"])
      .for_loops(&["for_statement"])
      .while_loops(&["while_statement"])
      .assignments(&["assignment"])
      .function_defs(&["function_definition"]);

    assert!(m.identifier_kinds.contains("identifier"));
    assert!(m.identifier_kinds.contains("name"));
    assert_eq!(m.literal_kinds.get("integer"), Some(&LiteralKind::Int));
    assert_eq!(m.binary_op_map.get("+"), Some(&BinOpKind::Add));
    assert!(m.skip_kinds.contains("comment"));
    assert!(m.block_kinds.contains("block"));
    assert!(m.call_kinds.contains("call"));
    assert!(m.return_kinds.contains("return_statement"));
    assert!(m.if_kinds.contains("if_statement"));
    assert!(m.for_kinds.contains("for_statement"));
    assert!(m.while_kinds.contains("while_statement"));
    assert!(m.assignment_kinds.contains("assignment"));
    assert!(m.function_def_kinds.contains("function_definition"));
  }

  // jscpd:ignore-end
}
