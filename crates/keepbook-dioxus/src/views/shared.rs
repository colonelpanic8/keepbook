use super::*;

/// Build [`SegmentedControl`] options from a view's `(preset, label)` table.
/// Each view offers its own preset subset and labels, but they all round-trip
/// through [`RangePreset::value`].
pub(super) fn range_preset_options(
    presets: &[(RangePreset, &'static str)],
) -> Vec<SegmentedOption> {
    presets
        .iter()
        .map(|(preset, label)| SegmentedOption::new(preset.value(), *label))
        .collect()
}

pub(super) fn range_preset_from_value(
    presets: &[(RangePreset, &'static str)],
    value: &str,
) -> Option<RangePreset> {
    presets
        .iter()
        .find(|(preset, _)| preset.value() == value)
        .map(|(preset, _)| *preset)
}
