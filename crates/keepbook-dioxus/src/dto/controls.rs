#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RangePreset {
    OneMonth,
    NinetyDays,
    SixMonths,
    OneYear,
    TwoYears,
    Max,
    Custom,
}

impl RangePreset {
    /// Stable identifier used to round-trip a preset through segmented-control
    /// option values.
    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::OneMonth => "one_month",
            Self::NinetyDays => "ninety_days",
            Self::SixMonths => "six_months",
            Self::OneYear => "one_year",
            Self::TwoYears => "two_years",
            Self::Max => "max",
            Self::Custom => "custom",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SamplingGranularity {
    Auto,
    Daily,
    Weekly,
    Monthly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SpendingBucket {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransactionSortField {
    Date,
    Amount,
    Description,
    Tag,
    Account,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AssetSortField {
    Name,
    Amount,
    AmountChecked,
    AmountChanged,
    PriceUpdated,
    Value,
    DayChange,
    WeekChange,
    MonthChange,
    YearChange,
}

impl AssetSortField {
    pub(crate) const OPTIONS: [Self; 10] = [
        Self::Name,
        Self::Amount,
        Self::Value,
        Self::PriceUpdated,
        Self::DayChange,
        Self::WeekChange,
        Self::MonthChange,
        Self::YearChange,
        Self::AmountChecked,
        Self::AmountChanged,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "Asset",
            Self::Amount => "Amount",
            Self::AmountChecked => "Amount checked",
            Self::AmountChanged => "Amount changed",
            Self::PriceUpdated => "Price updated",
            Self::Value => "Value",
            Self::DayChange => "1D change",
            Self::WeekChange => "1W change",
            Self::MonthChange => "1M change",
            Self::YearChange => "1Y change",
        }
    }

    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Amount => "amount",
            Self::AmountChecked => "amount_checked",
            Self::AmountChanged => "amount_changed",
            Self::PriceUpdated => "price_updated",
            Self::Value => "value",
            Self::DayChange => "day_change",
            Self::WeekChange => "week_change",
            Self::MonthChange => "month_change",
            Self::YearChange => "year_change",
        }
    }

    pub(crate) fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS
            .into_iter()
            .find(|field| field.value() == value)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SortDirection {
    Asc,
    Desc,
}

impl SortDirection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Asc => "Ascending",
            Self::Desc => "Descending",
        }
    }

    pub(crate) fn toggle(self) -> Self {
        match self {
            Self::Asc => Self::Desc,
            Self::Desc => Self::Asc,
        }
    }
}

impl SamplingGranularity {
    pub(crate) const OPTIONS: [Self; 5] = [
        Self::Auto,
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Yearly,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Yearly => "Yearly",
        }
    }

    pub(crate) fn query_value(self) -> &'static str {
        match self {
            Self::Auto => "daily",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    /// Distinct from [`Self::query_value`], which collapses `Auto` onto `Daily`
    /// for the backend query.
    pub(crate) fn value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Yearly => "yearly",
        }
    }

    pub(crate) fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS
            .into_iter()
            .find(|option| option.value() == value)
    }
}

impl SpendingBucket {
    pub(crate) const OPTIONS: [Self; 5] = [
        Self::Daily,
        Self::Weekly,
        Self::Monthly,
        Self::Quarterly,
        Self::Yearly,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Weekly => "Weekly",
            Self::Monthly => "Monthly",
            Self::Quarterly => "Quarterly",
            Self::Yearly => "Yearly",
        }
    }

    pub(crate) fn query_value(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
        }
    }

    pub(crate) fn from_value(value: &str) -> Option<Self> {
        Self::OPTIONS
            .into_iter()
            .find(|option| option.query_value() == value)
    }
}
