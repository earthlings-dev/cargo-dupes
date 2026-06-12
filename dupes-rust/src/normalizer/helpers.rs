use dupes_core::node::{
    BinOpKind, LiteralKind, NodeKind, NormalizationContext, NormalizedNode, PlaceholderKind,
    UnOpKind,
};
use syn::punctuated::Punctuated;

use super::expr::normalize_expr;

pub fn member_to_string(member: &syn::Member) -> String {
    match member {
        syn::Member::Named(ident) => ident.to_string(),
        syn::Member::Unnamed(idx) => idx.index.to_string(),
    }
}

/// Grammar position of a normalized node: expression, pattern, or type.
#[derive(Clone, Copy)]
pub enum PlaceholderNodeRole {
    Expr,
    Pat,
    Type,
}

pub fn placeholder_node(
    ctx: &mut NormalizationContext,
    name: &str,
    kind: PlaceholderKind,
    role: PlaceholderNodeRole,
) -> NormalizedNode {
    let idx = ctx.placeholder(name, kind);
    let node = match role {
        PlaceholderNodeRole::Expr => NodeKind::Placeholder(kind, idx),
        PlaceholderNodeRole::Pat => NodeKind::PatPlaceholder(kind, idx),
        PlaceholderNodeRole::Type => NodeKind::TypePlaceholder(kind, idx),
    };
    NormalizedNode::leaf(node)
}

pub fn path_segment_nodes(
    ctx: &mut NormalizationContext,
    path: &syn::Path,
    mut normalize_segment: impl FnMut(&mut NormalizationContext, &str) -> NormalizedNode,
) -> Vec<NormalizedNode> {
    path.segments
        .iter()
        .map(|seg| normalize_segment(ctx, &seg.ident.to_string()))
        .collect()
}

/// Normalize every path segment as one placeholder kind and role.
pub fn uniform_path_segment_nodes(
    ctx: &mut NormalizationContext,
    path: &syn::Path,
    kind: PlaceholderKind,
    role: PlaceholderNodeRole,
) -> Vec<NormalizedNode> {
    path_segment_nodes(ctx, path, |ctx, ident| {
        placeholder_node(ctx, ident, kind, role)
    })
}

/// Collapse a single-segment path to its segment node, or wrap in `multi_kind`.
pub fn path_node_from_segments(
    segments: Vec<NormalizedNode>,
    collapse_single: bool,
    multi_kind: NodeKind,
) -> NormalizedNode {
    if collapse_single && segments.len() == 1 {
        segments.into_iter().next().unwrap()
    } else {
        NormalizedNode::with_children(multi_kind, segments)
    }
}

/// Build a node by normalizing each item of a list as its children.
pub fn normalize_list<T>(
    kind: NodeKind,
    items: impl IntoIterator<Item = T>,
    ctx: &mut NormalizationContext,
    mut normalize: impl FnMut(T, &mut NormalizationContext) -> NormalizedNode,
) -> NormalizedNode {
    NormalizedNode::with_children(
        kind,
        items.into_iter().map(|item| normalize(item, ctx)).collect(),
    )
}

/// Build a node whose only child is the normalized `child`.
pub fn one_child_node<T: ?Sized>(
    kind: NodeKind,
    child: &T,
    ctx: &mut NormalizationContext,
    normalize: impl FnOnce(&T, &mut NormalizationContext) -> NormalizedNode,
) -> NormalizedNode {
    NormalizedNode::with_children(kind, vec![normalize(child, ctx)])
}

/// Build a reference-like node for the role, capturing mutability.
pub fn reference_node<T: ?Sized>(
    mutability: Option<&syn::token::Mut>,
    role: PlaceholderNodeRole,
    child: &T,
    ctx: &mut NormalizationContext,
    normalize: impl FnOnce(&T, &mut NormalizationContext) -> NormalizedNode,
) -> NormalizedNode {
    let mutable = mutability.is_some();
    let kind = match role {
        PlaceholderNodeRole::Expr => NodeKind::Reference { mutable },
        PlaceholderNodeRole::Pat => NodeKind::PatReference { mutable },
        PlaceholderNodeRole::Type => NodeKind::TypeReference { mutable },
    };
    NormalizedNode::with_children(kind, vec![normalize(child, ctx)])
}

pub fn normalize_macro(mac: &syn::Macro, ctx: &mut NormalizationContext) -> NormalizedNode {
    let name = mac
        .path
        .segments
        .last()
        .map(|s| s.ident.to_string())
        .unwrap_or_default();
    let args = if mac.tokens.is_empty() {
        Vec::new()
    } else {
        match mac.parse_body_with(Punctuated::<syn::Expr, syn::Token![,]>::parse_terminated) {
            Ok(punct) => punct.into_iter().map(|e| normalize_expr(&e, ctx)).collect(),
            Err(_) => vec![NormalizedNode::leaf(NodeKind::Opaque)],
        }
    };
    NormalizedNode::with_children(NodeKind::MacroCall { name }, args)
}

/// Build a literal node of the given kind with its value erased.
#[must_use]
pub const fn literal_node(kind: LiteralKind) -> NormalizedNode {
    NormalizedNode::leaf(NodeKind::Literal(kind))
}

#[must_use]
pub const fn normalize_lit(lit: &syn::Lit) -> NormalizedNode {
    match lit {
        syn::Lit::Str(_) => literal_node(LiteralKind::Str),
        syn::Lit::ByteStr(_) => literal_node(LiteralKind::ByteStr),
        syn::Lit::CStr(_) => literal_node(LiteralKind::CStr),
        syn::Lit::Byte(_) => literal_node(LiteralKind::Byte),
        syn::Lit::Char(_) => literal_node(LiteralKind::Char),
        syn::Lit::Int(_) => literal_node(LiteralKind::Int),
        syn::Lit::Float(_) => literal_node(LiteralKind::Float),
        syn::Lit::Bool(_) => literal_node(LiteralKind::Bool),
        _ => NormalizedNode::leaf(NodeKind::Opaque),
    }
}

#[must_use]
pub const fn normalize_bin_op(op: &syn::BinOp) -> BinOpKind {
    match op {
        syn::BinOp::Add(_) => BinOpKind::Add,
        syn::BinOp::Sub(_) => BinOpKind::Sub,
        syn::BinOp::Mul(_) => BinOpKind::Mul,
        syn::BinOp::Div(_) => BinOpKind::Div,
        syn::BinOp::Rem(_) => BinOpKind::Rem,
        syn::BinOp::And(_) => BinOpKind::And,
        syn::BinOp::Or(_) => BinOpKind::Or,
        syn::BinOp::BitXor(_) => BinOpKind::BitXor,
        syn::BinOp::BitAnd(_) => BinOpKind::BitAnd,
        syn::BinOp::BitOr(_) => BinOpKind::BitOr,
        syn::BinOp::Shl(_) => BinOpKind::Shl,
        syn::BinOp::Shr(_) => BinOpKind::Shr,
        syn::BinOp::Eq(_) => BinOpKind::Eq,
        syn::BinOp::Lt(_) => BinOpKind::Lt,
        syn::BinOp::Le(_) => BinOpKind::Le,
        syn::BinOp::Ne(_) => BinOpKind::Ne,
        syn::BinOp::Ge(_) => BinOpKind::Ge,
        syn::BinOp::Gt(_) => BinOpKind::Gt,
        syn::BinOp::AddAssign(_) => BinOpKind::AddAssign,
        syn::BinOp::SubAssign(_) => BinOpKind::SubAssign,
        syn::BinOp::MulAssign(_) => BinOpKind::MulAssign,
        syn::BinOp::DivAssign(_) => BinOpKind::DivAssign,
        syn::BinOp::RemAssign(_) => BinOpKind::RemAssign,
        syn::BinOp::BitXorAssign(_) => BinOpKind::BitXorAssign,
        syn::BinOp::BitAndAssign(_) => BinOpKind::BitAndAssign,
        syn::BinOp::BitOrAssign(_) => BinOpKind::BitOrAssign,
        syn::BinOp::ShlAssign(_) => BinOpKind::ShlAssign,
        syn::BinOp::ShrAssign(_) => BinOpKind::ShrAssign,
        _ => BinOpKind::Other,
    }
}

#[must_use]
pub const fn normalize_un_op(op: &syn::UnOp) -> UnOpKind {
    match op {
        syn::UnOp::Deref(_) => UnOpKind::Deref,
        syn::UnOp::Not(_) => UnOpKind::Not,
        syn::UnOp::Neg(_) => UnOpKind::Neg,
        _ => UnOpKind::Other,
    }
}
