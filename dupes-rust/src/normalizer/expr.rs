use dupes_core::node::{NodeKind, NormalizationContext, NormalizedNode, PlaceholderKind};

use super::helpers::{
    PlaceholderNodeRole, member_to_string, normalize_bin_op, normalize_list, normalize_lit,
    normalize_macro, normalize_un_op, one_child_node, path_node_from_segments, path_segment_nodes,
    placeholder_node, reference_node,
};
use super::pat::{normalize_pat, normalize_type};

#[allow(clippy::too_many_lines)]
pub fn normalize_expr(expr: &syn::Expr, ctx: &mut NormalizationContext) -> NormalizedNode {
    match expr {
        syn::Expr::Lit(el) => normalize_lit(&el.lit),
        syn::Expr::Path(ep) => normalize_expr_path(&ep.path, ctx),
        // BinaryOp -> [left, right]
        syn::Expr::Binary(eb) => NormalizedNode::with_children(
            NodeKind::BinaryOp(normalize_bin_op(&eb.op)),
            vec![
                normalize_expr(&eb.left, ctx),
                normalize_expr(&eb.right, ctx),
            ],
        ),
        // UnaryOp -> [operand]
        syn::Expr::Unary(eu) => NormalizedNode::with_children(
            NodeKind::UnaryOp(normalize_un_op(&eu.op)),
            vec![normalize_expr(&eu.expr, ctx)],
        ),
        // Call -> [func, arg0, arg1, ...]
        syn::Expr::Call(ec) => {
            let mut children = vec![normalize_expr(&ec.func, ctx)];
            children.extend(ec.args.iter().map(|a| normalize_expr(a, ctx)));
            NormalizedNode::with_children(NodeKind::Call, children)
        }
        // MethodCall -> [receiver, method, arg0, ...]
        // The method name is preserved as a Token leaf: which method runs is
        // behavior, not naming, so `x.is_ascii_alphabetic()` must never
        // fingerprint equal to `x.is_ascii_alphanumeric()`.
        syn::Expr::MethodCall(emc) => {
            let mut children = vec![
                normalize_expr(&emc.receiver, ctx),
                NormalizedNode::leaf(NodeKind::Token(emc.method.to_string())),
            ];
            children.extend(emc.args.iter().map(|a| normalize_expr(a, ctx)));
            NormalizedNode::with_children(NodeKind::MethodCall, children)
        }
        // FieldAccess -> [base, field]
        syn::Expr::Field(ef) => NormalizedNode::with_children(
            NodeKind::FieldAccess,
            vec![
                normalize_expr(&ef.base, ctx),
                placeholder_node(
                    ctx,
                    &member_to_string(&ef.member),
                    PlaceholderKind::Variable,
                    PlaceholderNodeRole::Expr,
                ),
            ],
        ),
        // Index -> [base, index]
        syn::Expr::Index(ei) => normalize_expr_pair(NodeKind::Index, &ei.expr, &ei.index, ctx),
        // Closure -> [body, param0, param1, ...]
        syn::Expr::Closure(ec) => {
            let mut children = vec![normalize_expr(&ec.body, ctx)];
            children.extend(ec.inputs.iter().map(|p| normalize_pat(p, ctx)));
            NormalizedNode::with_children(NodeKind::Closure, children)
        }
        // Return -> [] or [value]
        syn::Expr::Return(er) => node_with_optional_expr(NodeKind::Return, er.expr.as_deref(), ctx),
        // Break -> [] or [value]
        syn::Expr::Break(eb) => node_with_optional_expr(NodeKind::Break, eb.expr.as_deref(), ctx),
        syn::Expr::Continue(_) => NormalizedNode::leaf(NodeKind::Continue),
        // Assign -> [left, right]
        syn::Expr::Assign(ea) => normalize_expr_pair(NodeKind::Assign, &ea.left, &ea.right, ctx),
        // Reference -> [expr]
        syn::Expr::Reference(er) => reference_node(
            er.mutability.as_ref(),
            PlaceholderNodeRole::Expr,
            &*er.expr,
            ctx,
            normalize_expr,
        ),
        syn::Expr::Tuple(et) => normalize_list(NodeKind::Tuple, &et.elems, ctx, normalize_expr),
        syn::Expr::Array(ea) => normalize_list(NodeKind::Array, &ea.elems, ctx, normalize_expr),
        // Repeat -> [elem, len]
        syn::Expr::Repeat(er) => normalize_expr_pair(NodeKind::Repeat, &er.expr, &er.len, ctx),
        // Cast -> [expr, ty]
        syn::Expr::Cast(ec) => NormalizedNode::with_children(
            NodeKind::Cast,
            vec![normalize_expr(&ec.expr, ctx), normalize_type(&ec.ty, ctx)],
        ),
        // StructInit -> [rest_or_None, field0, field1, ...]
        syn::Expr::Struct(es) => {
            let mut children = vec![NormalizedNode::opt(
                es.rest.as_ref().map(|e| normalize_expr(e, ctx)),
            )];
            children.extend(es.fields.iter().map(|f| {
                let field_idx =
                    ctx.placeholder(&member_to_string(&f.member), PlaceholderKind::Variable);
                NormalizedNode::with_children(
                    NodeKind::FieldValue,
                    vec![
                        NormalizedNode::leaf(NodeKind::Placeholder(
                            PlaceholderKind::Variable,
                            field_idx,
                        )),
                        normalize_expr(&f.expr, ctx),
                    ],
                )
            }));
            NormalizedNode::with_children(NodeKind::StructInit, children)
        }
        // Await -> [expr]
        syn::Expr::Await(ea) => one_child_node(NodeKind::Await, &*ea.base, ctx, normalize_expr),
        // Try -> [expr]
        syn::Expr::Try(et) => one_child_node(NodeKind::Try, &*et.expr, ctx, normalize_expr),
        // If -> [condition, then_branch, else_or_None]
        syn::Expr::If(ei) => NormalizedNode::with_children(
            NodeKind::If,
            vec![
                normalize_expr(&ei.cond, ctx),
                normalize_block(&ei.then_branch, ctx),
                NormalizedNode::opt(ei.else_branch.as_ref().map(|(_, e)| normalize_expr(e, ctx))),
            ],
        ),
        // Match -> [expr, arm0, arm1, ...]
        // Each arm is MatchArm -> [pattern, guard_or_None, body]
        syn::Expr::Match(em) => {
            let mut children = vec![normalize_expr(&em.expr, ctx)];
            children.extend(em.arms.iter().map(|arm| {
                NormalizedNode::with_children(
                    NodeKind::MatchArm,
                    vec![
                        normalize_pat(&arm.pat, ctx),
                        NormalizedNode::opt(
                            arm.guard.as_ref().map(|(_, g)| normalize_expr(g, ctx)),
                        ),
                        normalize_expr(&arm.body, ctx),
                    ],
                )
            }));
            NormalizedNode::with_children(NodeKind::Match, children)
        }
        // Loop -> [body]
        syn::Expr::Loop(el) => one_child_node(NodeKind::Loop, &el.body, ctx, normalize_block),
        // While -> [condition, body]
        syn::Expr::While(ew) => NormalizedNode::with_children(
            NodeKind::While,
            vec![
                normalize_expr(&ew.cond, ctx),
                normalize_block(&ew.body, ctx),
            ],
        ),
        // ForLoop -> [pat, iter, body]
        syn::Expr::ForLoop(ef) => NormalizedNode::with_children(
            NodeKind::ForLoop,
            vec![
                normalize_pat(&ef.pat, ctx),
                normalize_expr(&ef.expr, ctx),
                normalize_block(&ef.body, ctx),
            ],
        ),
        syn::Expr::Block(eb) => normalize_block(&eb.block, ctx),
        // Paren -> [expr]
        syn::Expr::Paren(ep) => one_child_node(NodeKind::Paren, &*ep.expr, ctx, normalize_expr),
        // Range -> [from_or_None, to_or_None]
        syn::Expr::Range(er) => node_with_optional_expr_pair(
            NodeKind::Range,
            er.start.as_deref(),
            er.end.as_deref(),
            ctx,
        ),
        // LetExpr -> [pat, expr]
        syn::Expr::Let(el) => NormalizedNode::with_children(
            NodeKind::LetExpr,
            vec![normalize_pat(&el.pat, ctx), normalize_expr(&el.expr, ctx)],
        ),
        syn::Expr::Macro(em) => normalize_macro(&em.mac, ctx),
        syn::Expr::Group(eg) => normalize_expr(&eg.expr, ctx),
        syn::Expr::Unsafe(eu) => normalize_block(&eu.block, ctx),
        syn::Expr::Const(ec) => normalize_block(&ec.block, ctx),
        _ => NormalizedNode::leaf(NodeKind::Opaque),
    }
}

pub fn normalize_stmt(stmt: &syn::Stmt, ctx: &mut NormalizationContext) -> NormalizedNode {
    match stmt {
        // LetBinding -> [pattern, type_or_None, init_or_None, diverge_or_None]
        syn::Stmt::Local(local) => NormalizedNode::with_children(
            NodeKind::LetBinding,
            vec![
                normalize_pat(&local.pat, ctx),
                NormalizedNode::none(), // type annotations on let bindings are part of the pattern in syn
                NormalizedNode::opt(
                    local
                        .init
                        .as_ref()
                        .map(|init| normalize_expr(&init.expr, ctx)),
                ),
                NormalizedNode::opt(
                    local
                        .init
                        .as_ref()
                        .and_then(|init| init.diverge.as_ref())
                        .map(|(_, expr)| normalize_expr(expr, ctx)),
                ),
            ],
        ),
        syn::Stmt::Expr(expr, semi) => {
            let normalized = normalize_expr(expr, ctx);
            with_optional_semi(normalized, semi.is_some())
        }
        syn::Stmt::Item(_) => NormalizedNode::leaf(NodeKind::Opaque),
        syn::Stmt::Macro(sm) => {
            let normalized = normalize_macro(&sm.mac, ctx);
            with_optional_semi(normalized, sm.semi_token.is_some())
        }
    }
}

pub fn normalize_block(block: &syn::Block, ctx: &mut NormalizationContext) -> NormalizedNode {
    NormalizedNode::with_children(
        NodeKind::Block,
        block.stmts.iter().map(|s| normalize_stmt(s, ctx)).collect(),
    )
}

/// Build a node from a fixed pair of child expressions.
fn normalize_expr_pair(
    kind: NodeKind,
    left: &syn::Expr,
    right: &syn::Expr,
    ctx: &mut NormalizationContext,
) -> NormalizedNode {
    NormalizedNode::with_children(
        kind,
        vec![normalize_expr(left, ctx), normalize_expr(right, ctx)],
    )
}

/// Build a node whose children are an optional expression payload.
fn node_with_optional_expr(
    kind: NodeKind,
    expr: Option<&syn::Expr>,
    ctx: &mut NormalizationContext,
) -> NormalizedNode {
    let children = expr
        .map(|expr| vec![normalize_expr(expr, ctx)])
        .unwrap_or_default();
    NormalizedNode::with_children(kind, children)
}

/// Build a range-like node from optional start/end payloads, keeping the
/// `None` sentinel positions.
pub(super) fn node_with_optional_expr_pair(
    kind: NodeKind,
    start: Option<&syn::Expr>,
    end: Option<&syn::Expr>,
    ctx: &mut NormalizationContext,
) -> NormalizedNode {
    NormalizedNode::with_children(
        kind,
        vec![
            NormalizedNode::opt(start.map(|e| normalize_expr(e, ctx))),
            NormalizedNode::opt(end.map(|e| normalize_expr(e, ctx))),
        ],
    )
}

fn normalize_expr_path(path: &syn::Path, ctx: &mut NormalizationContext) -> NormalizedNode {
    let segments = path_segment_nodes(ctx, path, |ctx, ident| {
        let kind = if ident.chars().next().is_some_and(char::is_uppercase) {
            PlaceholderKind::Type
        } else {
            PlaceholderKind::Variable
        };
        placeholder_node(ctx, ident, kind, PlaceholderNodeRole::Expr)
    });

    path_node_from_segments(segments, true, NodeKind::Path)
}

fn with_optional_semi(normalized: NormalizedNode, has_semi: bool) -> NormalizedNode {
    if has_semi {
        NormalizedNode::with_children(NodeKind::Semi, vec![normalized])
    } else {
        normalized
    }
}
