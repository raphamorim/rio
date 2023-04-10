#[derive(Default, Debug)]
pub struct AccumulatedScroll {
    /// Scroll we should perform along `x` axis.
    pub x: f64,

    /// Scroll we should perform along `y` axis.
    pub y: f64,
}

#[derive(Debug, Default)]
pub struct Mouse {
    pub accumulated_scroll: AccumulatedScroll,
    pub multiplier: f64,
}
