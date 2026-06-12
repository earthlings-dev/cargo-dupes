// Frozen fixture: counts pinned in tests/detector_coverage.rs — update pins together with any edit.
use crate::Widget;

impl Widget {
    pub fn fresh() -> Self {
        Self { knobs: Vec::new() }
    }

    pub fn step(self, _knob: usize) -> Self {
        self
    }
}

/// Six-step builder run; the run lines repeat `build_two` verbatim while the
/// surrounding functions differ, so only the line dimension can pair them.
pub fn build_one() -> Widget {
    Widget::fresh()
        .alpha(1)
        .bravo(2)
        .charlie(3)
        .delta(4)
        .echo(5)
        .foxtrot(6)
}

/// Six-step builder run with a different receiver and tail than `build_one`.
pub fn build_two(seed: Widget) -> Widget {
    seed.step(0)
        .alpha(1)
        .bravo(2)
        .charlie(3)
        .delta(4)
        .echo(5)
        .foxtrot(6)
}

/// Detached chain fragment: too short to be a builder run, windows over it
/// stay chain-tail fragments.
pub fn tail_one(base: Widget) -> Widget {
    base
        .golf(7)
        .hotel(8)
        .india(9)
        .juliet(10)
}

/// Detached chain fragment with extra steps so the body never matches
/// `tail_one` in any AST dimension, exact or near.
pub fn tail_two(extra: Widget) -> Widget {
    extra
        .golf(7)
        .hotel(8)
        .india(9)
        .juliet(10)
        .kilo(11)
        .lima(12)
        .mike(13)
}
