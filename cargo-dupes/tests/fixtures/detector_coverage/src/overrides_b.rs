// Frozen fixture: counts pinned in tests/detector_coverage.rs — update pins together with any edit.
use crate::{Cfg, Ov};

/// Five-row option-override cluster; the chain differs from `apply_a` so the
/// chains never group, while every row repeats the same branch shape.
pub fn apply_b(cfg: &mut Cfg, ov: &Ov) {
    if let Some(alpha) = ov.alpha {
        cfg.alpha = alpha;
    }
    if let Some(nu) = ov.nu {
        cfg.nu = nu;
    }
    if let Some(beta) = ov.beta {
        cfg.beta = beta;
    }
    if let Some(xi) = ov.xi {
        cfg.xi = xi;
    }
    if let Some(gamma) = ov.gamma {
        cfg.gamma = gamma;
    }
}

/// Identical to `overrides_a::chain_x`: the whole chain groups as one unit.
/// The row bodies deliberately differ from the apply rows so chain coverage
/// fully hides this chain's branch group.
pub fn chain_y(cfg: &mut Cfg, ov: &Ov) {
    if let Some(kilo) = ov.kilo {
        cfg.kilo = kilo + 1;
    }
    if let Some(lima) = ov.lima {
        cfg.lima = lima + 1;
    }
    if let Some(mike) = ov.mike {
        cfg.mike = mike + 1;
    }
}
