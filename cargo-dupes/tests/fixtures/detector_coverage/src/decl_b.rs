// Frozen fixture: counts pinned in tests/detector_coverage.rs — update pins together with any edit.
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;

/// Clap-style options block: stanzas repeat `decl_a::CliA` verbatim.
pub struct CliB {
    /// Minimum alpha threshold applied before analysis.
    #[allow(dead_code)]
    pub alpha_limit: Option<usize>,

    /// Minimum beta threshold applied before analysis.
    #[allow(dead_code)]
    pub beta_limit: Option<usize>,

    /// Minimum gamma threshold applied before analysis.
    #[allow(dead_code)]
    pub gamma_limit: Option<usize>,

    /// Minimum delta threshold applied before analysis.
    #[allow(dead_code)]
    pub delta_limit: Option<usize>,

    /// Minimum epsilon threshold applied before analysis.
    #[allow(dead_code)]
    pub epsilon_limit: Option<usize>,

    /// Minimum zeta threshold applied before analysis.
    #[allow(dead_code)]
    pub zeta_limit: Option<usize>,
}

/// Blank-separated field table: rows repeat `decl_a::SpreadA` verbatim.
#[derive(Debug, Clone)]
pub struct SpreadB {
    pub first_total: usize,

    pub second_total: usize,

    pub third_total: usize,

    pub fourth_total: usize,

    pub fifth_total: usize,

    pub sixth_total: usize,
}

/// Contiguous field table: rows repeat `decl_a::DenseA` verbatim, so line
/// windows already see this pair.
#[derive(Debug, Clone)]
pub struct DenseB {
    pub one_count: usize,
    pub two_count: usize,
    pub three_count: usize,
    pub four_count: usize,
    pub five_count: usize,
    pub six_count: usize,
}

pub fn touch_b(map: &mut HashMap<String, usize>, set: &mut HashSet<String>) -> usize {
    map.len().max(set.len())
}

pub fn order_b(map: &mut BTreeMap<String, usize>, set: &mut BTreeSet<String>) -> usize {
    map.len().min(set.len())
}

pub fn queue_b(queue: &mut VecDeque<usize>) -> usize {
    queue.capacity()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert!(true);
    }
}
