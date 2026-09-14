use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum TuiViewArg {
    Transactions,
    NetWorth,
}

impl From<TuiViewArg> for keepbook::tui::TuiView {
    fn from(value: TuiViewArg) -> Self {
        match value {
            TuiViewArg::Transactions => keepbook::tui::TuiView::Transactions,
            TuiViewArg::NetWorth => keepbook::tui::TuiView::NetWorth,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum NetWorthIntervalArg {
    Full,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

impl From<NetWorthIntervalArg> for keepbook::tui::NetWorthInterval {
    fn from(value: NetWorthIntervalArg) -> Self {
        match value {
            NetWorthIntervalArg::Full => keepbook::tui::NetWorthInterval::Full,
            NetWorthIntervalArg::Hourly => keepbook::tui::NetWorthInterval::Hourly,
            NetWorthIntervalArg::Daily => keepbook::tui::NetWorthInterval::Daily,
            NetWorthIntervalArg::Weekly => keepbook::tui::NetWorthInterval::Weekly,
            NetWorthIntervalArg::Monthly => keepbook::tui::NetWorthInterval::Monthly,
            NetWorthIntervalArg::Yearly => keepbook::tui::NetWorthInterval::Yearly,
        }
    }
}
