// Frozen fixture: counts pinned in tests/detector_coverage.rs — update pins together with any edit.
use std::fmt::Write;

use crate::Cfg;

pub struct Gauge {
    pub level: usize,
    pub scale: usize,
    pub items: Vec<usize>,
    pub marks: Vec<usize>,
}

impl Gauge {
    /// Assignment builder setter: identical twin of `with_scale`.
    pub fn with_level(mut self, level: usize) -> Self {
        self.level = level;
        self
    }

    /// Assignment builder setter: identical twin of `with_level`.
    pub fn with_scale(mut self, scale: usize) -> Self {
        self.scale = scale;
        self
    }

    /// Method-mutation builder setter: identical twin of `push_mark`.
    pub fn push_item(mut self, item: usize) -> Self {
        self.items.push(item);
        self
    }

    /// Method-mutation builder setter: identical twin of `push_item`.
    pub fn push_mark(mut self, mark: usize) -> Self {
        self.marks.push(mark);
        self
    }

    /// Forwarding accessor: identical twin of `mark_for`.
    pub fn level_for(&self, span: usize) -> usize {
        self.scaled(span)
    }

    /// Forwarding accessor: identical twin of `level_for`.
    pub fn mark_for(&self, span: usize) -> usize {
        self.scaled(span)
    }

    fn scaled(&self, span: usize) -> usize {
        self.level * self.scale + span
    }
}

/// Small boolean projection: identical twin of `is_word_part`.
pub fn is_word_start(first: char) -> bool {
    first == '_' || first.is_ascii_alphabetic()
}

/// Small boolean projection: identical twin of `is_word_start`.
pub fn is_word_part(second: char) -> bool {
    second == '_' || second.is_ascii_alphabetic()
}

/// Large boolean projection (~30 nodes): identical twin of `spans_collide`.
pub fn ranges_overlap(a_start: usize, a_end: usize, b_start: usize, b_end: usize) -> bool {
    a_start.max(b_start) <= a_end.min(b_end)
        && a_start <= a_end
        && b_start <= b_end
        && a_start != b_end
        && b_start != a_end
}

/// Large boolean projection (~30 nodes): identical twin of `ranges_overlap`.
pub fn spans_collide(x_start: usize, x_end: usize, y_start: usize, y_end: usize) -> bool {
    x_start.max(y_start) <= x_end.min(y_end)
        && x_start <= x_end
        && y_start <= y_end
        && x_start != y_end
        && y_start != x_end
}

/// Comparator-adapter closure host; the closure is the twin, not the fn.
pub fn rank_items(values: &mut [Gauge]) {
    values.sort_by(|left, right| left.level.cmp(&right.level));
}

/// Comparator-adapter closure host with a different sort entry point.
pub fn rank_marks(values: &mut [Gauge]) {
    values.sort_unstable_by(|left, right| left.scale.cmp(&right.scale));
}

/// Message-only branch host: the `if verbose` branch twins with `render_sweep`.
pub fn render_pass(out: &mut String, verbose: bool, jobs: usize) -> std::fmt::Result {
    if verbose {
        writeln!(out, "starting pass")?;
    }
    let mut total = jobs;
    total += jobs / 2;
    writeln!(out, "{total}")
}

/// Message-only branch host with a different tail than `render_pass`.
pub fn render_sweep(out: &mut String, verbose: bool, lanes: usize) -> std::fmt::Result {
    if verbose {
        writeln!(out, "starting pass")?;
    }
    let wide = lanes * 3;
    let narrow = wide - lanes;
    writeln!(out, "{wide}/{narrow}")
}

/// Empty-default guard host: the guard branch twins with `collect_odds`.
pub fn collect_evens(values: &[usize]) -> Vec<usize> {
    if values.is_empty() {
        return Vec::new();
    }
    values.iter().filter(|value| **value % 2 == 0).copied().collect()
}

/// Empty-default guard host with a different filter than `collect_evens`.
pub fn collect_odds(values: &[usize]) -> Vec<usize> {
    if values.is_empty() {
        return Vec::new();
    }
    values.iter().filter(|value| **value % 2 != 0).copied().collect()
}

pub enum Kind {
    Alpha,
    Beta,
    Gamma,
    Delta,
}

/// Dispatch table: every arm body twins with every other arm body.
pub fn dispatch_first(kind: &Kind, cfg: &Cfg) -> usize {
    match kind {
        Kind::Alpha => alpha_total(cfg, kind, 1),
        Kind::Beta => beta_total(cfg, kind, 2),
        Kind::Gamma => gamma_total(cfg, kind, 3),
        Kind::Delta => delta_total(cfg, kind, 4),
    }
}

/// Dispatch table with a different arm count than `dispatch_first`.
pub fn dispatch_second(kind: &Kind, cfg: &Cfg) -> usize {
    match kind {
        Kind::Alpha => alpha_total(cfg, kind, 5),
        Kind::Beta => beta_total(cfg, kind, 6),
        Kind::Gamma => gamma_total(cfg, kind, 7),
    }
}

fn alpha_total(cfg: &Cfg, _kind: &Kind, bias: usize) -> usize {
    cfg.alpha + bias
}

fn beta_total(cfg: &Cfg, _kind: &Kind, bias: usize) -> usize {
    cfg.beta * bias
}

fn gamma_total(cfg: &Cfg, _kind: &Kind, bias: usize) -> usize {
    cfg.gamma - bias
}

fn delta_total(cfg: &Cfg, _kind: &Kind, bias: usize) -> usize {
    cfg.delta / bias
}

pub trait Renderer {
    fn render(
        &self,
        width: usize,
        depth: usize,
        emit_headers: bool,
        emit_footers: bool,
    ) -> String;
}

pub struct PlainRenderer;

impl Renderer for PlainRenderer {
    fn render(
        &self,
        width: usize,
        depth: usize,
        emit_headers: bool,
        emit_footers: bool,
    ) -> String {
        let mut out = String::new();
        if emit_headers {
            out.push('#');
        }
        out.push_str(&"-".repeat(width * depth));
        if emit_footers {
            out.push('.');
        }
        out
    }
}

pub struct FancyRenderer;

impl Renderer for FancyRenderer {
    fn render(
        &self,
        width: usize,
        depth: usize,
        emit_headers: bool,
        emit_footers: bool,
    ) -> String {
        let mut out = String::with_capacity(width + depth);
        out.push_str(&"=".repeat(width));
        if emit_headers {
            out.push('!');
        }
        out.push_str(&"~".repeat(depth));
        out
    }
}
