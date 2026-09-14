use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct History {
    pub(crate) currency: String,
    pub(crate) points: Vec<HistoryPoint>,
    /// Portfolio value at request time, present when the request asked for it
    /// and the range runs through today. `summary` covers `points` plus this.
    #[serde(default)]
    pub(crate) current: Option<HistoryPoint>,
    pub(crate) summary: Option<HistorySummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct HistoryPoint {
    pub(crate) date: String,
    pub(crate) total_value: String,
    pub(crate) percentage_change_from_previous: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct HistorySummary {
    pub(crate) initial_value: String,
    pub(crate) final_value: String,
    pub(crate) absolute_change: String,
    pub(crate) percentage_change: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct StackedHistory {
    pub(crate) currency: String,
    pub(crate) points: Vec<StackedHistoryPoint>,
    pub(crate) series: Vec<StackedHistorySeries>,
    #[serde(default)]
    pub(crate) current: Option<StackedHistoryPoint>,
    #[serde(default)]
    pub(crate) summary: Option<HistorySummary>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct StackedHistoryPoint {
    pub(crate) date: String,
    pub(crate) total_value: String,
    pub(crate) components: Vec<StackedHistoryComponent>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct StackedHistoryComponent {
    pub(crate) series_key: String,
    pub(crate) value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct StackedHistorySeries {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) series_type: String,
    pub(crate) account_id: Option<String>,
    pub(crate) account_name: Option<String>,
    pub(crate) connection_name: Option<String>,
    pub(crate) parent_key: Option<String>,
    pub(crate) asset: Option<serde_json::Value>,
}
