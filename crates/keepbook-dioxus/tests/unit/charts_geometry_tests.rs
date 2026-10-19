use super::*;

#[test]
fn stacked_layers_and_hover_regions_follow_date_spacing() {
    let data = ["2026-01-01", "2026-01-02", "2026-01-11"].map(|date| StackedHistoryDataPoint {
        date: date.into(),
        total: 100.0,
        components: vec![StackedValue {
            series_key: "cash".into(),
            value: 100.0,
        }],
    });
    let series = [ActiveStackedSeries {
        key: "cash".into(),
        label: "Cash".into(),
        account_id: None,
        series_type: "account".into(),
    }];
    let layers = build_stack_points(&data, &series, 68.0, 632.0);
    assert_eq!(
        layers.iter().map(|point| point.x).collect::<Vec<_>>(),
        vec![68.0, 131.2, 700.0]
    );
    let hover = stacked_hover_points(
        &data, &series, 0.0, 100.0, 68.0, 20.0, 18.0, 632.0, 244.0, 720.0,
    );
    assert_eq!(
        hover.iter().map(|point| point.x).collect::<Vec<_>>(),
        vec![68.0, 131.2, 700.0]
    );
    assert!((hover[1].hit_x - 99.6).abs() < 1e-9);
    assert!((hover[1].hit_x + hover[1].hit_width - 415.6).abs() < 1e-9);
}
