//! The language-agnostic normalized AST: [`NodeKind`] payloads, the
//! data-driven [`NormalizedNode`] tree, placeholder assignment and
//! re-indexing, and node counting.

use std::collections::HashMap;

/// Kinds of literals — preserves type but erases value.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralKind {
  /// Integer literal.
  Int,
  /// Floating-point literal.
  Float,
  /// String literal.
  Str,
  /// Byte-string literal.
  ByteStr,
  /// C-string literal.
  CStr,
  /// Byte literal.
  Byte,
  /// Character literal.
  Char,
  /// Boolean literal.
  Bool,
  /// Null-like literal (Python `None`).
  Null,
}

/// Kinds of placeholders — what the original identifier referred to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PlaceholderKind {
  /// A variable or parameter name.
  Variable,
  /// A function or method name.
  Function,
  /// A type name.
  Type,
  /// A lifetime name.
  Lifetime,
  /// A loop label.
  Label,
}

/// Binary operators.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BinOpKind {
  /// `+`
  Add,
  /// `-`
  Sub,
  /// `*`
  Mul,
  /// `/`
  Div,
  /// `%`
  Rem,
  /// `&&` / `and`
  And,
  /// `||` / `or`
  Or,
  /// `^`
  BitXor,
  /// `&`
  BitAnd,
  /// `|`
  BitOr,
  /// `<<`
  Shl,
  /// `>>`
  Shr,
  /// `==`
  Eq,
  /// `<`
  Lt,
  /// `<=`
  Le,
  /// `!=`
  Ne,
  /// `>=`
  Ge,
  /// `>`
  Gt,
  /// `+=`
  AddAssign,
  /// `-=`
  SubAssign,
  /// `*=`
  MulAssign,
  /// `/=`
  DivAssign,
  /// `%=`
  RemAssign,
  /// `^=`
  BitXorAssign,
  /// `&=`
  BitAndAssign,
  /// `|=`
  BitOrAssign,
  /// `<<=`
  ShlAssign,
  /// `>>=`
  ShrAssign,
  /// Python `//`
  FloorDiv,
  /// Python `**`
  Pow,
  /// Python `in`
  In,
  /// Python `not in`
  NotIn,
  /// Python `is`
  Is,
  /// Python `is not`
  IsNot,
  /// Python `//=`
  FloorDivAssign,
  /// Python `**=`
  PowAssign,
  /// Any binary operator not otherwise modeled.
  Other,
}

/// Unary operators.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum UnOpKind {
  /// `*` dereference.
  Deref,
  /// `!` / `not` logical negation.
  Not,
  /// `-` arithmetic negation.
  Neg,
  /// Any unary operator not otherwise modeled.
  Other,
}

/// The kind of a normalized AST node. Carries only non-child data
/// (operator kinds, literal kinds, placeholder indices, mutability flags, macro names).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum NodeKind {
  // Blocks and statements
  /// A statement block.
  Block,
  /// A `let` binding statement.
  LetBinding,
  /// An expression statement terminated by `;`.
  Semi,
  /// A parenthesized expression.
  Paren,

  // Literals and identifiers
  /// A literal, typed but value-erased.
  Literal(LiteralKind),
  /// A normalized identifier: its kind plus a per-kind placeholder index.
  Placeholder(PlaceholderKind, usize),

  // Operations
  /// A binary operation.
  BinaryOp(BinOpKind),
  /// A unary operation.
  UnaryOp(UnOpKind),
  /// A range expression.
  Range,

  // Calls and access
  /// A function call.
  Call,
  /// A method call.
  MethodCall,
  /// A field access.
  FieldAccess,
  /// An index expression.
  Index,
  /// A path expression.
  Path,

  // Closures and functions
  /// A closure expression.
  Closure,
  /// A function signature (return type plus parameters).
  FnSignature,

  // Control flow
  /// A `return` expression.
  Return,
  /// A `break` expression.
  Break,
  /// A `continue` expression.
  Continue,
  /// An assignment expression.
  Assign,

  // References and pointers
  /// A reference expression, tracking mutability.
  Reference {
    /// Whether the reference is mutable.
    mutable: bool,
  },

  // Compound types
  /// A tuple expression.
  Tuple,
  /// An array or list expression.
  Array,
  /// A set expression.
  Set,
  /// An array-repeat expression.
  Repeat,

  // Type operations
  /// A type cast.
  Cast,
  /// A struct or class initializer.
  StructInit,

  // Async/error
  /// An `await` expression.
  Await,
  /// A `yield` expression.
  Yield,
  /// A `?` try expression.
  Try,

  // Control flow structures
  /// An `if` expression.
  If,
  /// A `match` expression.
  Match,
  /// A single `match` arm.
  MatchArm,
  /// A bare `loop`.
  Loop,
  /// A `while` loop.
  While,
  /// A `for` loop.
  ForLoop,
  /// A `let` condition expression (`if let` / `while let`).
  LetExpr,

  // Patterns
  /// The wildcard pattern.
  PatWild,
  /// A normalized binding pattern (kind plus placeholder index).
  PatPlaceholder(PlaceholderKind, usize),
  /// A tuple pattern.
  PatTuple,
  /// A struct pattern.
  PatStruct,
  /// An or-pattern.
  PatOr,
  /// A literal pattern.
  PatLiteral,
  /// A reference pattern, tracking mutability.
  PatReference {
    /// Whether the pattern binds mutably.
    mutable: bool,
  },
  /// A slice pattern.
  PatSlice,
  /// A rest (`..`) pattern.
  PatRest,
  /// A range pattern.
  PatRange,

  // Types
  /// A normalized type identifier.
  TypePlaceholder(PlaceholderKind, usize),
  /// A reference type, tracking mutability.
  TypeReference {
    /// Whether the referenced type is mutable.
    mutable: bool,
  },
  /// A tuple type.
  TypeTuple,
  /// A slice type.
  TypeSlice,
  /// An array type.
  TypeArray,
  /// A path type.
  TypePath,
  /// An `impl Trait` type.
  TypeImplTrait,
  /// The inferred `_` type.
  TypeInfer,
  /// The unit type.
  TypeUnit,
  /// The never type.
  TypeNever,

  // Field initializer (name = value)
  /// A field initializer (`name: value`).
  FieldValue,

  // Macro invocations
  /// A macro invocation, keyed by the macro's name.
  MacroCall {
    /// The invoked macro's name.
    name: String,
  },

  // Generic token / line duplicate detection
  /// A source token from the generic token/line window dimensions.
  Token(String),

  /// An unsupported construct, erased to an opaque marker.
  Opaque,

  /// Sentinel for absent optional children, ensuring fixed child positions
  /// for correct zip alignment in similarity comparison.
  None,
}

/// A normalized AST node. Uses a data-driven `{ kind, children }` representation
/// instead of a large enum with differently-shaped variants. This allows generic
/// traversal algorithms (`count_nodes`, reindex, `count_matching`, extract) to work
/// without exhaustive matching on every variant.
///
/// ## Child ordering conventions
///
/// - **Fixed with None sentinels** (always same child count):
///   - `If` -> [condition, `then_branch`, `else_or_None`]
///   - `LetBinding` -> [pattern, `type_or_None`, `init_or_None`, `diverge_or_None`]
///   - `Range` / `PatRange` -> [`from_or_None`, `to_or_None`]
///   - `MatchArm` -> [pattern, `guard_or_None`, body]
/// - **Fixed children first, variable after** (for zip alignment):
///   - `Call` -> [func, arg0, arg1, ...]
///   - `MethodCall` -> [receiver, method, arg0, ...]
///   - `Closure` -> [body, param0, ...]
///   - `FnSignature` -> [`return_type_or_None`, param0, ...]
///   - `Match` -> [expr, arm0, arm1, ...]
///   - `StructInit` -> [`rest_or_None`, field0, field1, ...]
///   - `MacroCall` -> [arg0, arg1, ...]
/// - **Variable-length (0 or 1)**: `Return`, `Break` -> [] or [value]
/// - **Homogeneous**: `Block`, `Tuple`, `Array`, `Path`, `PatTuple`, etc. -> [elem0, ...]
/// - **All other fixed**: e.g. `BinaryOp` -> [left, right], `ForLoop` -> [pat, iter, body]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NormalizedNode {
  /// The node's kind payload (operator, literal, placeholder, ...).
  pub kind:     NodeKind,
  /// Ordered children, following the child ordering conventions above.
  pub children: Vec<Self>,
}

impl NormalizedNode {
  /// Create a leaf node (no children).
  #[must_use]
  pub const fn leaf(kind: NodeKind) -> Self {
    Self {
      kind,
      children: vec![],
    }
  }

  /// Create a node with children.
  #[must_use]
  pub const fn with_children(kind: NodeKind, children: Vec<Self>) -> Self {
    Self {
      kind,
      children,
    }
  }

  /// Create a None sentinel node.
  #[must_use]
  pub const fn none() -> Self {
    Self::leaf(NodeKind::None)
  }

  /// Convert an Option<NormalizedNode> to a node, using None sentinel for absent values.
  pub fn opt(node: Option<Self>) -> Self {
    node.unwrap_or_else(Self::none)
  }

  /// Check if this is a None sentinel node.
  #[must_use]
  pub const fn is_none(&self) -> bool {
    matches!(self.kind, NodeKind::None)
  }
}

/// Tracks identifier-to-placeholder mappings during normalization.
pub struct NormalizationContext {
  /// Maps (`identifier_string`, kind) -> placeholder index
  mappings: HashMap<(String, PlaceholderKind), usize>,
  /// Per-kind counters
  counters: HashMap<PlaceholderKind, usize>,
}

impl NormalizationContext {
  /// Create an empty context with no assigned placeholders.
  #[must_use]
  pub fn new() -> Self {
    Self {
      mappings: HashMap::new(),
      counters: HashMap::new(),
    }
  }

  /// Get or assign a placeholder index for the given identifier and kind.
  pub fn placeholder(&mut self, name: &str, kind: PlaceholderKind) -> usize {
    let key = (name.to_string(), kind);
    if let Some(&idx) = self.mappings.get(&key) {
      return idx;
    }
    let counter = self.counters.entry(kind).or_insert(0);
    let idx = *counter;
    *counter += 1;
    self.mappings.insert(key, idx);
    idx
  }
}

impl Default for NormalizationContext {
  fn default() -> Self {
    Self::new()
  }
}

// -- Placeholder re-indexing --------------------------------------------------

/// Collects all placeholder occurrences in depth-first order, building
/// a mapping from (kind, `old_index`) -> `new_sequential_index`.
fn collect_placeholder_order(
  node: &NormalizedNode,
  order: &mut Vec<(PlaceholderKind, usize)>,
  seen: &mut std::collections::HashSet<(PlaceholderKind, usize)>,
) {
  match &node.kind {
    NodeKind::Placeholder(kind, idx) | NodeKind::PatPlaceholder(kind, idx) | NodeKind::TypePlaceholder(kind, idx)
      if seen.insert((*kind, *idx)) =>
    {
      order.push((*kind, *idx));
    }
    _ => {}
  }
  for child in &node.children {
    collect_placeholder_order(child, order, seen);
  }
}

/// Applies the reindex mapping to a node, returning a new node with remapped indices.
fn apply_reindex(node: &NormalizedNode, mapping: &HashMap<(PlaceholderKind, usize), usize>) -> NormalizedNode {
  let kind = match &node.kind {
    NodeKind::Placeholder(kind, idx) => remap_placeholder(NodeKind::Placeholder, mapping, *kind, *idx),
    NodeKind::PatPlaceholder(kind, idx) => remap_placeholder(NodeKind::PatPlaceholder, mapping, *kind, *idx),
    NodeKind::TypePlaceholder(kind, idx) => remap_placeholder(NodeKind::TypePlaceholder, mapping, *kind, *idx),
    other => other.clone(),
  };
  let children = node.children.iter().map(|c| apply_reindex(c, mapping)).collect();
  NormalizedNode {
    kind,
    children,
  }
}

/// Rebuild a placeholder variant with its index remapped through `mapping`.
fn remap_placeholder(
  make: impl FnOnce(PlaceholderKind, usize) -> NodeKind,
  mapping: &HashMap<(PlaceholderKind, usize), usize>,
  kind: PlaceholderKind,
  idx: usize,
) -> NodeKind {
  make(kind, mapping.get(&(kind, idx)).copied().unwrap_or(idx))
}

/// Re-index all placeholders in a sub-tree so that indices start from 0
/// per kind, assigned by first-occurrence depth-first order.
/// This allows comparing sub-trees extracted from different function contexts.
#[must_use]
pub fn reindex_placeholders(node: &NormalizedNode) -> NormalizedNode {
  let mut order = Vec::new();
  let mut seen = std::collections::HashSet::new();
  collect_placeholder_order(node, &mut order, &mut seen);

  // Build mapping: (kind, old_index) -> new sequential index per kind
  let mut counters: HashMap<PlaceholderKind, usize> = HashMap::new();
  let mut mapping: HashMap<(PlaceholderKind, usize), usize> = HashMap::new();
  for (kind, old_idx) in order {
    let counter = counters.entry(kind).or_insert(0);
    mapping.insert((kind, old_idx), *counter);
    *counter += 1;
  }

  apply_reindex(node, &mapping)
}

/// Count the number of nodes in a normalized tree.
/// None sentinel nodes are not counted.
pub fn count_nodes(node: &NormalizedNode) -> usize {
  if node.is_none() {
    return 0;
  }
  1 + node.children.iter().map(count_nodes).sum::<usize>()
}

#[cfg(test)]
mod tests {
  use super::*;

  // jscpd:ignore-start

  #[test]
  fn reindex_remaps_from_zero() {
    let node = NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 5)),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 8)),
    ]);
    let reindexed = reindex_placeholders(&node);
    let expected = NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 1)),
    ]);
    assert_eq!(reindexed, expected);
  }

  #[test]
  fn reindex_preserves_same_placeholder_identity() {
    let node = NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 3)),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 3)),
    ]);
    let reindexed = reindex_placeholders(&node);
    let expected = NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
    ]);
    assert_eq!(reindexed, expected);
  }

  #[test]
  fn reindex_makes_equivalent_subtrees_equal() {
    let subtree1 = NormalizedNode::with_children(NodeKind::Block, vec![
      NormalizedNode::with_children(NodeKind::LetBinding, vec![
        NormalizedNode::leaf(NodeKind::PatPlaceholder(PlaceholderKind::Variable, 2)),
        NormalizedNode::none(),
        NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
          NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
          NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Int)),
        ]),
        NormalizedNode::none(),
      ]),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 2)),
    ]);
    let subtree2 = NormalizedNode::with_children(NodeKind::Block, vec![
      NormalizedNode::with_children(NodeKind::LetBinding, vec![
        NormalizedNode::leaf(NodeKind::PatPlaceholder(PlaceholderKind::Variable, 7)),
        NormalizedNode::none(),
        NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
          NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 5)),
          NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Int)),
        ]),
        NormalizedNode::none(),
      ]),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 7)),
    ]);

    assert_ne!(subtree1, subtree2);
    assert_eq!(reindex_placeholders(&subtree1), reindex_placeholders(&subtree2));
  }

  // jscpd:ignore-end

  #[test]
  fn reindex_handles_multiple_placeholder_kinds() {
    let node = NormalizedNode::with_children(NodeKind::Call, vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Function, 3)),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 5)),
      NormalizedNode::with_children(NodeKind::Cast, vec![
        NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 5)),
        NormalizedNode::leaf(NodeKind::TypePlaceholder(PlaceholderKind::Type, 2)),
      ]),
    ]);
    let reindexed = reindex_placeholders(&node);
    let expected = NormalizedNode::with_children(NodeKind::Call, vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Function, 0)),
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
      NormalizedNode::with_children(NodeKind::Cast, vec![
        NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
        NormalizedNode::leaf(NodeKind::TypePlaceholder(PlaceholderKind::Type, 0)),
      ]),
    ]);
    assert_eq!(reindexed, expected);
  }

  #[test]
  fn count_nodes_skips_none_sentinels() {
    let node = NormalizedNode::with_children(NodeKind::If, vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
      NormalizedNode::with_children(NodeKind::Block, vec![]),
      NormalizedNode::none(),
    ]);
    // If(1) + Placeholder(1) + Block(1) = 3 (None is not counted)
    assert_eq!(count_nodes(&node), 3);
  }

  // -- NormalizationContext tests --

  #[test]
  fn context_assigns_sequential_indices() {
    let mut ctx = NormalizationContext::new();
    assert_eq!(ctx.placeholder("x", PlaceholderKind::Variable), 0);
    assert_eq!(ctx.placeholder("y", PlaceholderKind::Variable), 1);
    assert_eq!(ctx.placeholder("z", PlaceholderKind::Variable), 2);
  }

  #[test]
  fn context_returns_same_index_for_same_name() {
    let mut ctx = NormalizationContext::new();
    let first = ctx.placeholder("x", PlaceholderKind::Variable);
    let second = ctx.placeholder("x", PlaceholderKind::Variable);
    assert_eq!(first, second);
    assert_eq!(first, 0);
  }

  #[test]
  fn context_per_kind_counters_are_independent() {
    let mut ctx = NormalizationContext::new();
    let var_idx = ctx.placeholder("foo", PlaceholderKind::Variable);
    let fn_idx = ctx.placeholder("foo", PlaceholderKind::Function);
    let type_idx = ctx.placeholder("foo", PlaceholderKind::Type);
    // Each kind starts from 0 independently
    assert_eq!(var_idx, 0);
    assert_eq!(fn_idx, 0);
    assert_eq!(type_idx, 0);
  }

  #[test]
  fn context_same_name_different_kind_are_distinct() {
    let mut ctx = NormalizationContext::new();
    ctx.placeholder("x", PlaceholderKind::Variable);
    ctx.placeholder("x", PlaceholderKind::Function);
    // Second variable should get index 1, not 0
    let y_var = ctx.placeholder("y", PlaceholderKind::Variable);
    assert_eq!(y_var, 1);
    let y_fn = ctx.placeholder("y", PlaceholderKind::Function);
    assert_eq!(y_fn, 1);
  }

  // -- count_nodes tests --

  #[test]
  fn count_nodes_basic() {
    let node = NormalizedNode::with_children(NodeKind::BinaryOp(BinOpKind::Add), vec![
      NormalizedNode::leaf(NodeKind::Placeholder(PlaceholderKind::Variable, 0)),
      NormalizedNode::leaf(NodeKind::Literal(LiteralKind::Int)),
    ]);
    assert_eq!(count_nodes(&node), 3);
  }
}
