// Frozen fixture: counts pinned in tests/detector_coverage.rs — update pins together with any edit.
pub mod builders;
pub mod decl_a;
pub mod decl_b;
pub mod overrides_a;
pub mod overrides_b;
pub mod shapes;

pub struct Cfg {
    pub alpha: usize,
    pub beta: usize,
    pub gamma: usize,
    pub delta: usize,
    pub epsilon: usize,
    pub zeta: usize,
    pub eta: usize,
    pub theta: usize,
    pub iota: usize,
    pub kilo: usize,
    pub lima: usize,
    pub mike: usize,
    pub nu: usize,
    pub xi: usize,
}

pub struct Ov {
    pub alpha: Option<usize>,
    pub beta: Option<usize>,
    pub gamma: Option<usize>,
    pub delta: Option<usize>,
    pub epsilon: Option<usize>,
    pub zeta: Option<usize>,
    pub eta: Option<usize>,
    pub theta: Option<usize>,
    pub iota: Option<usize>,
    pub kilo: Option<usize>,
    pub lima: Option<usize>,
    pub mike: Option<usize>,
    pub nu: Option<usize>,
    pub xi: Option<usize>,
}

pub struct Widget {
    pub knobs: Vec<usize>,
}
