/// The categorical `--series-N` palette shared by chart series and spending
/// tags, assigned by position and wrapping past the last slot.
pub(crate) fn series_color(index: usize) -> &'static str {
    const PALETTE: [&str; 10] = [
        "var(--series-1)",
        "var(--series-2)",
        "var(--series-3)",
        "var(--series-4)",
        "var(--series-5)",
        "var(--series-6)",
        "var(--series-7)",
        "var(--series-8)",
        "var(--series-9)",
        "var(--series-10)",
    ];
    PALETTE[index % PALETTE.len()]
}
