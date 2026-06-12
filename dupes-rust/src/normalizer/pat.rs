use dupes_core::node::{NodeKind, NormalizationContext, NormalizedNode, PlaceholderKind};

use super::expr::{node_with_optional_expr_pair, normalize_expr};
use super::helpers::{
    PlaceholderNodeRole, member_to_string, normalize_list, normalize_lit, normalize_macro,
    one_child_node, path_node_from_segments, placeholder_node, reference_node,
    uniform_path_segment_nodes,
};

pub fn normalize_type(ty: &syn::Type, ctx: &mut NormalizationContext) -> NormalizedNode {
    match ty {
        syn::Type::Path(tp) => normalize_type_path(&tp.path, tp.qself.is_none(), ctx),
        syn::Type::Reference(r) => reference_node(
            r.mutability.as_ref(),
            PlaceholderNodeRole::Type,
            &*r.elem,
            ctx,
            normalize_type,
        ),
        syn::Type::Tuple(t) => {
            if t.elems.is_empty() {
                NormalizedNode::leaf(NodeKind::TypeUnit)
            } else {
                normalize_list(NodeKind::TypeTuple, &t.elems, ctx, normalize_type)
            }
        }
        syn::Type::Slice(s) => one_child_node(NodeKind::TypeSlice, &*s.elem, ctx, normalize_type),
        syn::Type::Array(a) => NormalizedNode::with_children(
            NodeKind::TypeArray,
            vec![normalize_type(&a.elem, ctx), normalize_expr(&a.len, ctx)],
        ),
        syn::Type::ImplTrait(i) => NormalizedNode::with_children(
            NodeKind::TypeImplTrait,
            i.bounds
                .iter()
                .filter_map(|b| {
                    if let syn::TypeParamBound::Trait(t) = b {
                        Some(normalize_type_path(&t.path, true, ctx))
                    } else {
                        None
                    }
                })
                .collect(),
        ),
        syn::Type::Infer(_) => NormalizedNode::leaf(NodeKind::TypeInfer),
        syn::Type::Never(_) => NormalizedNode::leaf(NodeKind::TypeNever),
        syn::Type::Paren(p) => normalize_type(&p.elem, ctx),
        syn::Type::Macro(tm) => normalize_macro(&tm.mac, ctx),
        _ => NormalizedNode::leaf(NodeKind::Opaque),
    }
}

pub fn normalize_pat(pat: &syn::Pat, ctx: &mut NormalizationContext) -> NormalizedNode {
    match pat {
        syn::Pat::Ident(pi) => placeholder_node(
            ctx,
            &pi.ident.to_string(),
            PlaceholderKind::Variable,
            PlaceholderNodeRole::Pat,
        ),
        syn::Pat::Wild(_) => NormalizedNode::leaf(NodeKind::PatWild),
        syn::Pat::Tuple(pt) => normalize_list(NodeKind::PatTuple, &pt.elems, ctx, normalize_pat),
        syn::Pat::TupleStruct(pts) => {
            normalize_list(NodeKind::PatStruct, &pts.elems, ctx, normalize_pat)
        }
        syn::Pat::Struct(ps) => NormalizedNode::with_children(
            NodeKind::PatStruct,
            ps.fields
                .iter()
                .map(|f| {
                    let value = normalize_pat(&f.pat, ctx);
                    NormalizedNode::with_children(
                        NodeKind::FieldValue,
                        vec![
                            placeholder_node(
                                ctx,
                                &member_to_string(&f.member),
                                PlaceholderKind::Variable,
                                PlaceholderNodeRole::Pat,
                            ),
                            value,
                        ],
                    )
                })
                .collect(),
        ),
        syn::Pat::Or(po) => normalize_list(NodeKind::PatOr, &po.cases, ctx, normalize_pat),
        syn::Pat::Lit(pl) => {
            NormalizedNode::with_children(NodeKind::PatLiteral, vec![normalize_lit(&pl.lit)])
        }
        syn::Pat::Reference(pr) => reference_node(
            pr.mutability.as_ref(),
            PlaceholderNodeRole::Pat,
            &*pr.pat,
            ctx,
            normalize_pat,
        ),
        syn::Pat::Slice(ps) => normalize_list(NodeKind::PatSlice, &ps.elems, ctx, normalize_pat),
        syn::Pat::Rest(_) => NormalizedNode::leaf(NodeKind::PatRest),
        // PatRange -> [from_or_None, to_or_None]
        syn::Pat::Range(pr) => node_with_optional_expr_pair(
            NodeKind::PatRange,
            pr.start.as_deref(),
            pr.end.as_deref(),
            ctx,
        ),
        syn::Pat::Path(pp) => normalize_pat_path(&pp.path, ctx),
        syn::Pat::Type(pt) => normalize_pat(&pt.pat, ctx),
        syn::Pat::Macro(pm) => normalize_macro(&pm.mac, ctx),
        _ => NormalizedNode::leaf(NodeKind::Opaque),
    }
}

fn normalize_type_path(
    path: &syn::Path,
    single_segment_as_placeholder: bool,
    ctx: &mut NormalizationContext,
) -> NormalizedNode {
    let segments =
        uniform_path_segment_nodes(ctx, path, PlaceholderKind::Type, PlaceholderNodeRole::Type);
    path_node_from_segments(segments, single_segment_as_placeholder, NodeKind::TypePath)
}

fn normalize_pat_path(path: &syn::Path, ctx: &mut NormalizationContext) -> NormalizedNode {
    let segments = uniform_path_segment_nodes(
        ctx,
        path,
        PlaceholderKind::Variable,
        PlaceholderNodeRole::Pat,
    );
    path_node_from_segments(segments, true, NodeKind::PatStruct)
}
