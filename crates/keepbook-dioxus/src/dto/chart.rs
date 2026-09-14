#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PieSlice {
    pub(crate) key: String,
    pub(crate) total: f64,
    pub(crate) transaction_count: usize,
    pub(crate) percentage: f64,
    pub(crate) path: String,
    pub(crate) color: &'static str,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct NetWorthDataPoint {
    pub(crate) date: String,
    pub(crate) value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StackedHistoryDataPoint {
    pub(crate) date: String,
    pub(crate) total: f64,
    pub(crate) components: Vec<StackedValue>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StackedValue {
    pub(crate) series_key: String,
    pub(crate) value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ActiveStackedSeries {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) account_id: Option<String>,
    pub(crate) series_type: String,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpendingBarChartPoint {
    pub(crate) label: String,
    pub(crate) start_date: String,
    pub(crate) end_date: String,
    pub(crate) total: f64,
    pub(crate) transaction_count: usize,
    pub(crate) segments: Vec<SpendingBarSegment>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpendingBarSegment {
    pub(crate) key: String,
    pub(crate) value: f64,
    pub(crate) transaction_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SpendingPeriodSelection {
    pub(crate) label: String,
    pub(crate) start_date: String,
    pub(crate) end_date: String,
    pub(crate) total: f64,
    pub(crate) transaction_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChartPoint {
    pub(crate) date: String,
    pub(crate) value: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChartHoverPoint {
    pub(crate) index: usize,
    pub(crate) point: ChartPoint,
    pub(crate) hit_x: f64,
    pub(crate) hit_width: f64,
}
