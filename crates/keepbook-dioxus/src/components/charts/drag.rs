//! Drag-to-select helpers shared by the line and stacked charts.

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChartDragSelection {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) width: f64,
    pub(crate) height: f64,
}

pub(crate) fn chart_drag_selection(
    start: Option<usize>,
    current: Option<usize>,
    point_xs: Vec<f64>,
    padding_top: f64,
    plot_height: f64,
) -> Option<ChartDragSelection> {
    let start = start?;
    let current = current?;
    if start == current {
        return None;
    }
    let start_x = *point_xs.get(start)?;
    let current_x = *point_xs.get(current)?;
    let x = start_x.min(current_x);
    Some(ChartDragSelection {
        x,
        y: padding_top,
        width: (start_x - current_x).abs().max(1.0),
        height: plot_height,
    })
}

pub(crate) fn selected_date_range(
    start: Option<usize>,
    current: Option<usize>,
    dates: &[String],
) -> Option<(String, String)> {
    let start = start?;
    let current = current?;
    if start == current {
        return None;
    }
    let min_index = start.min(current);
    let max_index = start.max(current);
    Some((dates.get(min_index)?.clone(), dates.get(max_index)?.clone()))
}
