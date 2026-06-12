// Frozen fixture: counts pinned in tests/detector_coverage.rs — update pins together with any edit.
use crate::{Cfg, Ov};

/// Nine-row option-override cluster; every row shares its branch shape with
/// the rows of `overrides_b::apply_b` even though the chains differ.
pub fn apply_a(cfg: &mut Cfg, ov: &Ov) {
    if let Some(alpha) = ov.alpha {
        cfg.alpha = alpha;
    }
    if let Some(beta) = ov.beta {
        cfg.beta = beta;
    }
    if let Some(gamma) = ov.gamma {
        cfg.gamma = gamma;
    }
    if let Some(delta) = ov.delta {
        cfg.delta = delta;
    }
    if let Some(epsilon) = ov.epsilon {
        cfg.epsilon = epsilon;
    }
    if let Some(zeta) = ov.zeta {
        cfg.zeta = zeta;
    }
    if let Some(eta) = ov.eta {
        cfg.eta = eta;
    }
    if let Some(theta) = ov.theta {
        cfg.theta = theta;
    }
    if let Some(iota) = ov.iota {
        cfg.iota = iota;
    }
}

/// Identical to `overrides_b::chain_y`: the whole chain groups as one unit.
/// The row bodies deliberately differ from the apply rows so chain coverage
/// fully hides this chain's branch group.
pub fn chain_x(cfg: &mut Cfg, ov: &Ov) {
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
