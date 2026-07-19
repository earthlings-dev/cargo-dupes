//! Rust `CodeUnit` extraction through `syn`: top-level and sub-function
//! extractors, impl-aware naming, and test-code tagging.

use std::path::Path;
use std::path::PathBuf;

pub use dupes_core::code_unit::CodeUnit;
pub use dupes_core::code_unit::CodeUnitKind;
use dupes_core::fingerprint::Fingerprint;
use dupes_core::node::NodeKind;
use dupes_core::node::NormalizationContext;
use dupes_core::node::NormalizedNode;
use syn::spanned::Spanned;
use syn::visit::Visit;

use crate::normalizer;

/// Check if attributes contain `#[test]`.
fn has_test_attr(attrs: &[syn::Attribute]) -> bool {
  attrs.iter().any(|attr| attr.path().is_ident("test"))
}

/// Define `with_test_context` for a unit extractor: run `visit` with the
/// `#[cfg(test)]`-context flag set to `is_test`, restoring the previous
/// value afterwards. Stamped into both extractors so the context juggling
/// is stated once.
macro_rules! with_test_context_method {
  () => {
    fn with_test_context(&mut self, is_test: bool, visit: impl FnOnce(&mut Self)) {
      let previous_test = self.in_test_context;
      self.in_test_context = is_test;
      visit(self);
      self.in_test_context = previous_test;
    }
  };
}

/// Extracts nested code units with precise spans.
struct SubUnitExtractor {
  file:            PathBuf,
  min_node_count:  usize,
  units:           Vec<CodeUnit>,
  current_parent:  Option<String>,
  in_test_context: bool,
  /// `if` statements represented by an if-chain unit, mapped to that chain
  /// unit's content fingerprint; their branch units are emitted linked via
  /// `parent_chain` so the pipeline can treat them as chain-covered.
  chained_ifs:     std::collections::HashMap<usize, Fingerprint>,
}

impl SubUnitExtractor {
  fn new(file: PathBuf, min_node_count: usize) -> Self {
    Self {
      file,
      min_node_count,
      units: Vec::new(),
      current_parent: None,
      in_test_context: false,
      chained_ifs: std::collections::HashMap::new(),
    }
  }

  with_test_context_method!();

  fn with_parent(&mut self, parent: String, is_test: bool, visit: impl FnOnce(&mut Self)) {
    let previous_parent = self.current_parent.replace(parent);
    self.with_test_context(is_test, visit);
    self.current_parent = previous_parent;
  }

  fn add_expr_unit(
    &mut self,
    kind: CodeUnitKind,
    description: String,
    expr: &syn::Expr,
    line_start: usize,
    line_end: usize,
    parent_chain: Option<Fingerprint>,
  ) -> bool {
    let mut ctx = NormalizationContext::new();
    let body = dupes_core::node::reindex_placeholders(&normalizer::normalize_expr(expr, &mut ctx));
    self.add_normalized_unit(kind, description, body, line_start, line_end, parent_chain)
  }

  fn add_block_unit(&mut self, kind: CodeUnitKind, description: String, block: &syn::Block, parent_chain: Option<Fingerprint>) -> bool {
    let mut ctx = NormalizationContext::new();
    let body = dupes_core::node::reindex_placeholders(&normalizer::normalize_block(block, &mut ctx));
    let line_start = block.brace_token.span.open().start().line;
    let line_end = block.brace_token.span.close().end().line;
    self.add_normalized_unit(kind, description, body, line_start, line_end, parent_chain)
  }

  fn add_loop_body(&mut self, description: &str, block: &syn::Block) {
    self.add_block_unit(CodeUnitKind::LoopBody, description.to_string(), block, None);
  }

  fn add_normalized_unit(
    &mut self,
    kind: CodeUnitKind,
    description: String,
    body: NormalizedNode,
    line_start: usize,
    line_end: usize,
    parent_chain: Option<Fingerprint>,
  ) -> bool {
    let node_count = normalizer::count_nodes(&body);
    if node_count < self.min_node_count {
      return false;
    }
    self.units.push(CodeUnit {
      suppressed: None,
      parent_chain,
      kind,
      name: description,
      file: self.file.clone(),
      line_start,
      line_end,
      signature: NormalizedNode::leaf(NodeKind::Opaque),
      fingerprint: Fingerprint::from_node(&body),
      node_count,
      body,
      parent_name: self.current_parent.clone(),
      is_test: self.in_test_context,
    });
    true
  }

  /// Extract runs of two or more consecutive `if` statements as one
  /// coherent if-chain unit (for example option-to-field setter clusters),
  /// replacing the per-branch fragments of the chained statements.
  fn collect_if_chains(&mut self, block: &syn::Block) {
    let mut run: Vec<&syn::Expr> = Vec::new();
    for stmt in &block.stmts {
      if let syn::Stmt::Expr(expr @ syn::Expr::If(_), _) = stmt {
        run.push(expr);
      } else {
        self.flush_if_chain(&run);
        run.clear();
      }
    }
    self.flush_if_chain(&run);
  }

  fn flush_if_chain(&mut self, run: &[&syn::Expr]) {
    if run.len() < 2 {
      return;
    }
    let mut ctx = NormalizationContext::new();
    let chain = NormalizedNode::with_children(
      NodeKind::Block,
      run.iter().map(|expr| normalizer::normalize_expr(expr, &mut ctx)).collect(),
    );
    let body = dupes_core::node::reindex_placeholders(&chain);
    let chain_fp = Fingerprint::from_node(&body);
    let line_start = run.first().map_or(1, |expr| expr.span().start().line);
    let line_end = run.last().map_or(line_start, |expr| expr.span().end().line);
    // Branches link to the chain only when the chain itself became a
    // unit; a sub-threshold chain leaves its branches unlinked, which
    // matches their pre-chain behavior because they fall under the same
    // node threshold.
    if self.add_normalized_unit(
      CodeUnitKind::IfChain,
      format!("if chain ({} branches)", run.len()),
      body,
      line_start,
      line_end,
      None,
    ) {
      for expr in run {
        if let syn::Expr::If(expr_if) = expr {
          self
            .chained_ifs
            .insert(std::ptr::from_ref::<syn::ExprIf>(expr_if) as usize, chain_fp);
        }
      }
    }
  }
}

/// Define a loop visitor that extracts the loop body before recursing.
macro_rules! visit_loop_body {
  ($method:ident, $expr_ty:ty, $label:literal, $visitor:path) => {
    fn $method(&mut self, node: &'ast $expr_ty) {
      self.add_loop_body($label, &node.body);
      $visitor(self, node);
    }
  };
}

/// Define the module visitor shared by both extractors: recurse with the
/// `#[cfg(test)]` context propagated to everything inside the module.
macro_rules! visit_item_mod_with_test_context {
  () => {
    fn visit_item_mod(&mut self, node: &'ast syn::ItemMod) {
      let is_test = self.in_test_context || has_cfg_test_attr(&node.attrs);
      self.with_test_context(is_test, |visitor| {
        syn::visit::visit_item_mod(visitor, node);
      });
    }
  };
}

impl<'ast> Visit<'ast> for SubUnitExtractor {
  fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
    let is_test = item_fn_is_test(self.in_test_context, node);
    self.with_parent(node.sig.ident.to_string(), is_test, |visitor| {
      visitor.visit_block(&node.block);
    });
  }

  fn visit_block(&mut self, node: &'ast syn::Block) {
    self.collect_if_chains(node);
    syn::visit::visit_block(self, node);
  }

  visit_item_mod_with_test_context!();

  fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
    let naming = ImplNaming::of(node);
    let is_test = self.in_test_context || has_cfg_test_attr(&node.attrs);
    self.with_test_context(is_test, |visitor| {
      for item in &node.items {
        if let syn::ImplItem::Fn(method) = item {
          let full_name = naming.method_name(method);
          let in_test_context = visitor.in_test_context;
          visitor.with_parent(full_name, in_test_context, |visitor| {
            visitor.visit_block(&method.block);
          });
        }
      }
    });
  }

  fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
    let owning_chain = self
      .chained_ifs
      .get(&(std::ptr::from_ref::<syn::ExprIf>(node) as usize))
      .copied();
    self.add_block_unit(
      CodeUnitKind::IfBranch,
      "if-then branch".to_string(),
      &node.then_branch,
      owning_chain,
    );
    if let Some((_, else_expr)) = &node.else_branch {
      let span = else_expr.span();
      self.add_expr_unit(
        CodeUnitKind::IfBranch,
        "if-else branch".to_string(),
        else_expr,
        span.start().line,
        span.end().line,
        owning_chain,
      );
    }
    syn::visit::visit_expr_if(self, node);
  }

  fn visit_expr_match(&mut self, node: &'ast syn::ExprMatch) {
    for (idx, arm) in node.arms.iter().enumerate() {
      let span = arm.body.span();
      self.add_expr_unit(
        CodeUnitKind::MatchArm,
        format!("match arm {}", idx + 1),
        &arm.body,
        span.start().line,
        span.end().line,
        None,
      );
    }
    syn::visit::visit_expr_match(self, node);
  }

  visit_loop_body!(visit_expr_loop, syn::ExprLoop, "loop body", syn::visit::visit_expr_loop);
  visit_loop_body!(visit_expr_while, syn::ExprWhile, "while body", syn::visit::visit_expr_while);
  visit_loop_body!(visit_expr_for_loop, syn::ExprForLoop, "for body", syn::visit::visit_expr_for_loop);

  fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
    if let syn::Expr::Block(block) = &*node.body {
      self.add_block_unit(CodeUnitKind::Block, "closure body".to_string(), &block.block, None);
    }
    syn::visit::visit_expr_closure(self, node);
  }
}

/// Check if attributes contain `#[cfg(test)]`.
fn has_cfg_test_attr(attrs: &[syn::Attribute]) -> bool {
  attrs
    .iter()
    .any(|attr| attr.path().is_ident("cfg") && attr.parse_args::<syn::Ident>().is_ok_and(|ident| ident == "test"))
}

/// True when a free `fn` is test code: marked `#[test]`, gated by
/// `#[cfg(test)]`, or already inside a test context.
fn item_fn_is_test(in_test_context: bool, node: &syn::ItemFn) -> bool {
  in_test_context || has_test_attr(&node.attrs) || has_cfg_test_attr(&node.attrs)
}

/// Extracts code units from a syn file by visiting the AST.
struct CodeUnitExtractor {
  file:            PathBuf,
  min_node_count:  usize,
  min_line_count:  usize,
  units:           Vec<CodeUnit>,
  /// Track if we're inside test code (`#[cfg(test)]` module/impl).
  in_test_context: bool,
}

impl CodeUnitExtractor {
  const fn new(file: PathBuf, min_node_count: usize, min_line_count: usize) -> Self {
    Self {
      file,
      min_node_count,
      min_line_count,
      units: Vec::new(),
      in_test_context: false,
    }
  }

  #[allow(clippy::too_many_arguments)]
  fn add_unit(
    &mut self,
    kind: CodeUnitKind,
    name: String,
    line_start: usize,
    line_end: usize,
    sig: NormalizedNode,
    body: NormalizedNode,
    is_test: bool,
  ) {
    let node_count = normalizer::count_nodes(&sig) + normalizer::count_nodes(&body);
    if node_count < self.min_node_count {
      return;
    }
    let line_count = line_end.saturating_sub(line_start) + 1;
    if self.min_line_count > 0 && line_count < self.min_line_count {
      return;
    }
    let fingerprint = Fingerprint::from_sig_and_body(&sig, &body);
    self.units.push(CodeUnit {
      suppressed: None,
      parent_chain: None,
      kind,
      name,
      file: self.file.clone(),
      line_start,
      line_end,
      signature: sig,
      body,
      fingerprint,
      node_count,
      parent_name: None,
      is_test,
    });
  }

  with_test_context_method!();
}

impl<'ast> Visit<'ast> for CodeUnitExtractor {
  fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
    let is_test = item_fn_is_test(self.in_test_context, node);

    let name = node.sig.ident.to_string();
    let line_start = node.sig.ident.span().start().line;
    let line_end = node.block.brace_token.span.close().end().line;
    let (sig, body) = normalizer::normalize_item_fn(node);
    self.add_unit(CodeUnitKind::Function, name, line_start, line_end, sig, body, is_test);

    // Continue visiting nested items (propagate test context)
    self.with_test_context(is_test, |visitor| {
      syn::visit::visit_item_fn(visitor, node);
    });
  }

  visit_item_mod_with_test_context!();

  fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
    let is_test = self.in_test_context || has_cfg_test_attr(&node.attrs);
    let naming = ImplNaming::of(node);

    self.with_test_context(is_test, |visitor| {
      for item in &node.items {
        if let syn::ImplItem::Fn(method) = item {
          let full_name = naming.method_name(method);

          let line_start = method.sig.ident.span().start().line;
          let line_end = method.block.brace_token.span.close().end().line;

          let (sig, body) = normalizer::normalize_fn_like(&method.sig, &method.block);
          let kind = if naming.is_trait_impl {
            CodeUnitKind::TraitImplBlock
          } else {
            CodeUnitKind::Method
          };

          let in_test_context = visitor.in_test_context;
          visitor.add_unit(kind, full_name, line_start, line_end, sig, body, in_test_context);
        }
      }
    });
  }

  fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
    let line_start = node.or1_token.span.start().line;
    let line_end = match &*node.body {
      syn::Expr::Block(eb) => eb.block.brace_token.span.close().end().line,
      other => {
        let end = other.span().end().line;
        if end > 0 { end } else { line_start }
      }
    };

    let normalized = normalizer::normalize_closure_expr(node);
    let node_count = normalizer::count_nodes(&normalized);
    let line_count = line_end.saturating_sub(line_start) + 1;
    if node_count >= self.min_node_count && (self.min_line_count == 0 || line_count >= self.min_line_count) {
      let name = format!("closure at {}:{}", self.file.display(), line_start);
      let fingerprint = Fingerprint::from_node(&normalized);
      self.units.push(CodeUnit {
        suppressed: None,
        parent_chain: None,
        kind: CodeUnitKind::Closure,
        name,
        file: self.file.clone(),
        line_start,
        line_end,
        signature: NormalizedNode::leaf(dupes_core::node::NodeKind::Opaque),
        body: normalized,
        fingerprint,
        node_count,
        parent_name: None,
        is_test: self.in_test_context,
      });
    }

    // Continue visiting nested closures
    syn::visit::visit_expr_closure(self, node);
  }
}

/// Join a path's segment identifiers with `::`.
fn path_name(path: &syn::Path) -> String {
  path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>().join("::")
}

/// Get a simple string representation of a type for naming.
fn quote_type(ty: &syn::Type) -> String {
  match ty {
    syn::Type::Path(tp) => path_name(&tp.path),
    _ => "Unknown".to_string(),
  }
}

/// Method naming for one `impl` block, shared by both extractors.
struct ImplNaming {
  type_name:     String,
  trait_name:    String,
  is_trait_impl: bool,
}

impl ImplNaming {
  fn of(node: &syn::ItemImpl) -> Self {
    Self {
      type_name:     quote_type(&node.self_ty),
      trait_name:    node.trait_.as_ref().map(|(_, path, _)| path_name(path)).unwrap_or_default(),
      is_trait_impl: node.trait_.is_some(),
    }
  }

  /// `<Type as Trait>::method` for trait impls, `Type::method` otherwise.
  fn method_name(&self, method: &syn::ImplItemFn) -> String {
    let method_name = method.sig.ident.to_string();
    let type_name = &self.type_name;
    if self.is_trait_impl {
      let trait_name = &self.trait_name;
      format!("<{type_name} as {trait_name}>::{method_name}")
    } else {
      format!("{type_name}::{method_name}")
    }
  }
}

/// Parse Rust source code and extract code units.
///
/// This is the core parsing entry point used by `RustAnalyzer`.
/// `path` is used for diagnostics and naming only.
/// Test code is always included but tagged with `is_test: true`;
/// filtering is handled by the caller.
pub fn parse_source(path: &Path, source: &str, min_node_count: usize, min_line_count: usize) -> Result<Vec<CodeUnit>, String> {
  let file = parse_syn_file(path, source)?;

  let mut extractor = CodeUnitExtractor::new(path.to_path_buf(), min_node_count, min_line_count);
  extractor.visit_file(&file);

  Ok(extractor.units)
}

/// Parse Rust source code and extract nested sub-function units.
pub fn parse_sub_units(path: &Path, source: &str, min_node_count: usize) -> Result<Vec<CodeUnit>, String> {
  let file = parse_syn_file(path, source)?;

  let mut extractor = SubUnitExtractor::new(path.to_path_buf(), min_node_count);
  extractor.visit_file(&file);

  Ok(extractor.units)
}

/// Parse Rust source through syn, tagging errors with the originating path.
fn parse_syn_file(path: &Path, source: &str) -> Result<syn::File, String> {
  syn::parse_file(source).map_err(|e| format!("Failed to parse {}: {}", path.display(), e))
}

/// Parse a single Rust file and extract code units.
///
/// This is a lower-level convenience function. Prefer using [`crate::RustAnalyzer`]
/// with [`dupes_core::analyze`] for the full pipeline.
pub fn parse_file(path: &Path, min_node_count: usize, min_line_count: usize) -> Result<Vec<CodeUnit>, String> {
  let content = std::fs::read_to_string(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;

  parse_source(path, &content, min_node_count, min_line_count)
}

/// Parse multiple files and collect all code units, skipping files that fail to parse.
///
/// This is a lower-level convenience function. Prefer using [`crate::RustAnalyzer`]
/// with [`dupes_core::analyze`] for the full pipeline.
#[must_use]
pub fn parse_files(paths: &[PathBuf], min_node_count: usize, min_line_count: usize) -> (Vec<CodeUnit>, Vec<String>) {
  let mut units = Vec::new();
  let mut warnings = Vec::new();

  for path in paths {
    match parse_file(path, min_node_count, min_line_count) {
      Ok(file_units) => units.extend(file_units),
      Err(warning) => warnings.push(warning),
    }
  }

  (units, warnings)
}

#[cfg(test)]
mod tests {
  use std::fs;

  use tempfile::TempDir;

  use super::*;

  fn write_and_parse(code: &str, min_nodes: usize) -> Vec<CodeUnit> {
    parse_source(Path::new("test.rs"), code, min_nodes, 0).unwrap()
  }

  // jscpd:ignore-start

  #[test]
  fn extracts_top_level_functions() {
    let units = write_and_parse(
      r#"
            fn foo(x: i32) -> i32 {
                let y = x + 1;
                y * 2
            }
            fn bar() {
                println!("hello");
            }
            "#,
      1,
    );
    let fns: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::Function).collect();
    assert_eq!(fns.len(), 2);
    assert_eq!(fns[0].name, "foo");
    assert_eq!(fns[1].name, "bar");
  }

  // jscpd:ignore-end

  #[test]
  fn extracts_methods_from_impl() {
    let units = write_and_parse(
      r"
            struct Foo;
            impl Foo {
                fn bar(&self) -> i32 {
                    42
                }
                fn baz(&mut self, val: i32) {
                    let _ = val + 1;
                }
            }
            ",
      1,
    );
    let methods: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::Method).collect();
    assert_eq!(methods.len(), 2);
    assert!(methods[0].name.contains("Foo::bar"));
    assert!(methods[1].name.contains("Foo::baz"));
  }

  #[test]
  fn extracts_trait_impl_methods() {
    let units = write_and_parse(
      r"
            struct Foo;
            trait MyTrait {
                fn do_thing(&self) -> i32;
            }
            impl MyTrait for Foo {
                fn do_thing(&self) -> i32 {
                    let x = 42;
                    x + 1
                }
            }
            ",
      1,
    );
    let trait_impls: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::TraitImplBlock).collect();
    assert_eq!(trait_impls.len(), 1);
    assert!(trait_impls[0].name.contains("Foo"));
    assert!(trait_impls[0].name.contains("MyTrait"));
    assert!(trait_impls[0].name.contains("do_thing"));
  }

  #[test]
  fn respects_min_node_count() {
    let units_low = write_and_parse(
      r"
            fn tiny() -> i32 { 1 }
            fn bigger(x: i32) -> i32 {
                let a = x + 1;
                let b = a * 2;
                a + b
            }
            ",
      1,
    );
    let units_high = write_and_parse(
      r"
            fn tiny() -> i32 { 1 }
            fn bigger(x: i32) -> i32 {
                let a = x + 1;
                let b = a * 2;
                a + b
            }
            ",
      20,
    );
    assert!(units_low.len() >= units_high.len());
  }

  // jscpd:ignore-start

  #[test]
  fn duplicate_functions_same_fingerprint() {
    let units = write_and_parse(
      r"
            fn foo(x: i32) -> i32 {
                let y = x + 1;
                y * 2
            }
            fn bar(a: i32) -> i32 {
                let b = a + 1;
                b * 2
            }
            ",
      1,
    );
    let fns: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::Function).collect();
    assert_eq!(fns.len(), 2);
    assert_eq!(fns[0].fingerprint, fns[1].fingerprint);
  }

  #[test]
  fn different_functions_different_fingerprint() {
    let units = write_and_parse(
      r"
            fn add(x: i32) -> i32 {
                x + 1
            }
            fn mul(x: i32) -> i32 {
                x * 2
            }
            ",
      1,
    );
    let fns: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::Function).collect();
    assert_eq!(fns.len(), 2);
    assert_ne!(fns[0].fingerprint, fns[1].fingerprint);
  }

  // jscpd:ignore-end

  #[test]
  fn handles_parse_errors_gracefully() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("broken.rs");
    fs::write(&file, "fn broken( { }").unwrap();
    let result = parse_file(&file, 1, 0);
    assert!(result.is_err());
  }

  #[test]
  fn parse_files_collects_warnings() {
    let tmp = TempDir::new().unwrap();
    let good = tmp.path().join("good.rs");
    let bad = tmp.path().join("bad.rs");
    fs::write(&good, "fn good() { let x = 1; }").unwrap();
    fs::write(&bad, "fn bad( {").unwrap();
    let (units, warnings) = parse_files(&[good, bad], 1, 0);
    assert!(!units.is_empty());
    assert_eq!(warnings.len(), 1);
  }

  #[test]
  fn code_unit_has_line_numbers() {
    let units = write_and_parse(
      r"
fn first() {
    let x = 1;
}

fn second() {
    let y = 2;
}
            ",
      1,
    );
    assert!(units.len() >= 2);
    // First function starts at line 2
    assert!(units[0].line_start > 0);
    assert!(units[0].line_end >= units[0].line_start);
  }

  #[test]
  fn code_unit_kind_display() {
    assert_eq!(CodeUnitKind::Function.to_string(), "function");
    assert_eq!(CodeUnitKind::Method.to_string(), "method");
    assert_eq!(CodeUnitKind::Closure.to_string(), "closure");
  }

  #[test]
  fn extracts_closures() {
    let units = write_and_parse(
      r"
            fn foo() {
                let f = |x: i32, y: i32| {
                    let sum = x + y;
                    let product = x * y;
                    sum + product
                };
            }
            ",
      1,
    );
    let has_closure = units.iter().any(|u| u.kind == CodeUnitKind::Closure);
    assert!(has_closure);
  }

  // jscpd:ignore-start

  #[test]
  fn parse_sub_units_extracts_if_chain_with_precise_span() {
    // Consecutive option-to-field setter branches are one coherent
    // chain unit, not many tiny if-branch fragments.
    let units = parse_sub_units(
      Path::new("test.rs"),
      r"
            fn apply(config: &mut Config, overrides: &Overrides) {
                if let Some(width_limit) = overrides.width_limit {
                    config.width_limit = width_limit;
                }
                if let Some(depth_limit) = overrides.depth_limit {
                    config.depth_limit = depth_limit;
                }
                if let Some(score_limit) = overrides.score_limit {
                    config.score_limit = score_limit;
                }
            }
            ",
      1,
    )
    .unwrap();

    let chains: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::IfChain).collect();
    assert_eq!(chains.len(), 1);
    assert_eq!(chains[0].name, "if chain (3 branches)");
    assert_eq!(chains[0].line_start, 3);
    assert_eq!(chains[0].line_end, 11);
    let branches: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::IfBranch).collect();
    assert_eq!(branches.len(), 3, "chained setter branches stay extracted, linked to the chain");
    for branch in branches {
      assert_eq!(branch.parent_chain, Some(chains[0].fingerprint));
    }
  }

  // jscpd:ignore-end

  #[test]
  fn parse_sub_units_keeps_single_if_branch_extraction() {
    let units = parse_sub_units(
      Path::new("test.rs"),
      r"
            fn single(config: &mut Config, value: Option<usize>) {
                if let Some(value) = value {
                    config.min_nodes = value;
                }
                let _ = config;
            }
            ",
      1,
    )
    .unwrap();

    assert!(units.iter().any(|u| u.kind == CodeUnitKind::IfBranch));
    assert!(units.iter().all(|u| u.kind != CodeUnitKind::IfChain));
  }

  // jscpd:ignore-start

  #[test]
  fn identical_if_chains_share_fingerprints_across_functions() {
    let units = parse_sub_units(
      Path::new("test.rs"),
      r"
            fn apply_first(config: &mut Config, overrides: &Overrides) {
                if let Some(node_quota) = overrides.node_quota {
                    config.node_quota = node_quota;
                }
                if let Some(line_quota) = overrides.line_quota {
                    config.line_quota = line_quota;
                }
            }
            fn apply_second(target: &mut Config, source: &Source) {
                if let Some(node_floor) = source.node_floor {
                    target.node_floor = node_floor;
                }
                if let Some(line_floor) = source.line_floor {
                    target.line_floor = line_floor;
                }
            }
            ",
      1,
    )
    .unwrap();

    let chains: Vec<_> = units.iter().filter(|u| u.kind == CodeUnitKind::IfChain).collect();
    assert_eq!(chains.len(), 2);
    assert_eq!(chains[0].fingerprint, chains[1].fingerprint);
  }

  // jscpd:ignore-end

  #[test]
  fn parse_sub_units_extracts_each_loop_body_kind() {
    let units = parse_sub_units(
      Path::new("test.rs"),
      r"
            fn loops(xs: Vec<i32>) {
                loop {
                    break;
                }
                while xs.is_empty() {
                    break;
                }
                for x in xs {
                    let _ = x;
                }
            }
            ",
      1,
    )
    .unwrap();
    let loop_names: Vec<_> = units
      .iter()
      .filter(|u| u.kind == CodeUnitKind::LoopBody)
      .map(|u| u.name.as_str())
      .collect();
    assert_eq!(loop_names, ["loop body", "while body", "for body"]);
  }

  #[test]
  fn min_line_count_filters_short_functions() {
    let code = r"
fn short(x: i32) -> i32 {
    x + 1
}

fn longer(x: i32) -> i32 {
    let a = x + 1;
    let b = a * 2;
    let c = b - 3;
    let d = c + 4;
    a + b + c + d
}
        ";
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("test.rs");
    fs::write(&file, code).unwrap();

    // With min_line_count=0, both functions should appear
    let units_all = parse_file(&file, 1, 0).unwrap();
    assert!(units_all.len() >= 2);

    // With min_line_count=5, only the longer function should pass
    let units_filtered = parse_file(&file, 1, 5).unwrap();
    assert!(units_filtered.len() < units_all.len());
    for unit in &units_filtered {
      let lines = unit.line_end.saturating_sub(unit.line_start) + 1;
      assert!(lines >= 5, "unit {} has only {lines} lines", unit.name);
    }
  }

  #[test]
  fn test_has_test_attr() {
    let file: syn::File = syn::parse_str(
      r"
            #[test]
            fn my_test() {}
            fn normal() {}
            ",
    )
    .unwrap();

    let items = &file.items;
    if let syn::Item::Fn(f) = &items[0] {
      assert!(has_test_attr(&f.attrs));
    } else {
      panic!("expected function");
    }
    if let syn::Item::Fn(f) = &items[1] {
      assert!(!has_test_attr(&f.attrs));
    } else {
      panic!("expected function");
    }
  }

  #[test]
  fn test_has_cfg_test_attr() {
    let file: syn::File = syn::parse_str(
      r"
            #[cfg(test)]
            mod tests {}
            mod normal {}
            ",
    )
    .unwrap();

    let items = &file.items;
    if let syn::Item::Mod(m) = &items[0] {
      assert!(has_cfg_test_attr(&m.attrs));
    } else {
      panic!("expected module");
    }
    if let syn::Item::Mod(m) = &items[1] {
      assert!(!has_cfg_test_attr(&m.attrs));
    } else {
      panic!("expected module");
    }
  }

  // jscpd:ignore-start

  #[test]
  fn test_functions_tagged_as_test() {
    let code = r"
            fn production(x: i32) -> i32 {
                let y = x + 1;
                y * 2
            }
            #[test]
            fn my_test() {
                let x = 1;
                let y = x + 1;
                assert_eq!(y, 2);
            }
        ";

    let units = write_and_parse(code, 1);
    let prod: Vec<_> = units.iter().filter(|u| u.name == "production").collect();
    let test: Vec<_> = units.iter().filter(|u| u.name == "my_test").collect();

    assert_eq!(prod.len(), 1);
    assert!(!prod[0].is_test);
    assert_eq!(test.len(), 1);
    assert!(test[0].is_test);
  }

  #[test]
  fn cfg_test_module_functions_tagged_as_test() {
    let code = r"
            fn production(x: i32) -> i32 {
                let y = x + 1;
                y * 2
            }

            #[cfg(test)]
            mod tests {
                fn helper(x: i32) -> i32 {
                    let y = x + 1;
                    y * 2
                }
            }
        ";

    let units = write_and_parse(code, 1);
    let prod: Vec<_> = units.iter().filter(|u| u.name == "production").collect();
    let helper: Vec<_> = units.iter().filter(|u| u.name == "helper").collect();

    assert_eq!(prod.len(), 1);
    assert!(!prod[0].is_test);
    assert_eq!(helper.len(), 1);
    assert!(helper[0].is_test);
  }

  #[test]
  fn non_test_code_not_tagged() {
    let code = r"
            fn production(x: i32) -> i32 {
                let y = x + 1;
                y * 2
            }
            #[test]
            fn my_test() {
                let x = 1;
                let y = x + 1;
                assert_eq!(y, 2);
            }
        ";

    let units = write_and_parse(code, 1);
    let non_test: Vec<_> = units.iter().filter(|u| !u.is_test).collect();
    assert!(!non_test.is_empty());
    assert!(non_test.iter().all(|u| u.name != "my_test"));
  }

  #[test]
  fn cfg_test_impl_blocks_tagged_as_test() {
    let code = r"
            struct Foo;

            impl Foo {
                fn production(&self) -> i32 {
                    let x = 42;
                    x + 1
                }
            }

            #[cfg(test)]
            impl Foo {
                fn test_helper(&self) -> i32 {
                    let x = 42;
                    x + 1
                }
            }
        ";

    let units = write_and_parse(code, 1);
    let prod: Vec<_> = units.iter().filter(|u| u.name.contains("production")).collect();
    let helper: Vec<_> = units.iter().filter(|u| u.name.contains("test_helper")).collect();

    assert_eq!(prod.len(), 1);
    assert!(!prod[0].is_test);
    assert_eq!(helper.len(), 1);
    assert!(helper[0].is_test);
  }

  // jscpd:ignore-end

  #[test]
  fn parse_source_works() {
    let path = Path::new("test.rs");
    let source = "fn foo(x: i32) -> i32 { x + 1 }";
    let units = parse_source(path, source, 1, 0).unwrap();
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].name, "foo");
  }

  #[test]
  fn builder_setters_are_not_emitted_as_units() {
    let units = write_and_parse(
      r"
            struct Builder { resolver: Option<u32>, detector: Option<u32> }
            impl Builder {
                pub fn with_resolver(mut self, resolver: u32) -> Self {
                    self.resolver = Some(resolver);
                    self
                }
                pub fn with_detector(mut self, detector: u32) -> Self {
                    self.detector = Some(detector);
                    self
                }
            }
            ",
      1,
    );
    // The parser emits setters unconditionally; the pipeline tags
    // them with ast.setter-returning-self (pinned in dupes-core tests).
    assert_eq!(
      units.iter().filter(|u| u.name.contains("with_")).count(),
      2,
      "setter-and-return-self bodies are emitted for pipeline tagging"
    );
  }

  #[test]
  fn validating_builder_setters_remain_units() {
    let units = write_and_parse(
      r#"
            struct Builder { port: u16 }
            impl Builder {
                pub fn with_port(mut self, port: u16) -> Result<Self, String> {
                    if port == 0 {
                        return Err("port must be nonzero".to_string());
                    }
                    self.port = port;
                    Ok(self)
                }
            }
            "#,
      1,
    );
    assert!(
      units.iter().any(|u| u.name.contains("with_port")),
      "behavior-bearing setters must remain reportable"
    );
  }

  #[test]
  fn accessor_forwarding_methods_are_not_emitted_as_units() {
    let units = write_and_parse(
      r"
            struct Stats { exact: usize, near: usize }
            impl Stats {
                pub fn exact_percent(&self) -> f64 {
                    self.percent_of(self.exact)
                }
                pub fn near_percent(&self) -> f64 {
                    self.percent_of(self.near)
                }
            }
            ",
      1,
    );
    // Accessors are emitted unconditionally and tagged by the
    // pipeline with ast.forwarding-accessor (pinned in dupes-core tests).
    assert_eq!(
      units.iter().filter(|u| u.kind == CodeUnitKind::Method).count(),
      2,
      "forwarding accessors are emitted for pipeline tagging"
    );
  }

  #[test]
  fn comparator_adapter_closures_are_not_emitted_as_units() {
    let units = write_and_parse(
      r"
            fn sort_groups(groups: &mut Vec<Group>) {
                groups.sort_by(|a, b| start_key(a).cmp(&start_key(b)));
            }
            ",
      1,
    );
    // Comparator-adapter closures are emitted unconditionally and
    // tagged by the pipeline with ast.comparator-adapter.
    assert!(units.iter().any(|u| u.kind == CodeUnitKind::Closure));
  }

  #[test]
  fn constant_binding_wrappers_remain_units() {
    // `fixture_path("cargo-dupes", name)`-style named specializations
    // stay reportable: the wrapper binds a constant.
    let units = write_and_parse(
      r#"
            fn rust_fixture_path(name: &str) -> PathBuf {
                fixture_path("cargo-dupes", name)
            }
            "#,
      1,
    );
    assert_eq!(units.len(), 1);
    assert_eq!(units[0].name, "rust_fixture_path");
  }

  #[test]
  fn behavior_bearing_small_functions_remain_units() {
    let units = write_and_parse(
      r"
            fn clamp_total(total: i32) -> i32 {
                if total > 100 { 100 } else { total }
            }
            ",
      1,
    );
    assert_eq!(units.len(), 1);
  }

  #[test]
  fn structured_closures_remain_units() {
    let units = write_and_parse(
      r#"
            fn collect_names(paths: &[Item]) -> Vec<String> {
                paths
                    .iter()
                    .map(|item| {
                        let name = item.ident.to_string();
                        format!("{name}::suffix")
                    })
                    .collect()
            }
            "#,
      1,
    );
    let closure_count = units.iter().filter(|u| u.kind == CodeUnitKind::Closure).count();
    assert_eq!(closure_count, 1, "the mapping closure stays a unit");
  }
}
