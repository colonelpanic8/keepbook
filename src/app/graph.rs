use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

use crate::app::types::HistoryOutput;
use crate::app::{
    latent_capital_gains_tax_history, portfolio_history, portfolio_history_for_accounts,
    resolve_portfolio_history_selection, PortfolioHistorySelection,
};
use crate::config::ResolvedConfig;
use crate::storage::Storage;

const DEFAULT_WIDTH: u32 = 1400;
const DEFAULT_HEIGHT: u32 = 900;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
struct GraphConfigFile {
    start: Option<String>,
    end: Option<String>,
    currency: Option<String>,
    granularity: Option<String>,
    include_prices: Option<bool>,
    account: Option<String>,
    connection: Option<String>,
    output: Option<PathBuf>,
    svg_output: Option<PathBuf>,
    title: Option<String>,
    subtitle: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    min_value: Option<f64>,
    max_value: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct PortfolioGraphOptions {
    pub graph_config: Option<PathBuf>,
    pub start: Option<String>,
    pub end: Option<String>,
    pub currency: Option<String>,
    pub granularity: Option<String>,
    pub include_prices: Option<bool>,
    pub account: Option<String>,
    pub connection: Option<String>,
    pub output: Option<PathBuf>,
    pub svg_output: Option<PathBuf>,
    pub title: Option<String>,
    pub subtitle: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub min_value: Option<f64>,
    pub max_value: Option<f64>,
}

#[derive(Debug, Clone)]
struct ResolvedGraphOptions {
    start: Option<String>,
    end: Option<String>,
    currency: Option<String>,
    granularity: String,
    include_prices: bool,
    account: Option<String>,
    connection: Option<String>,
    output: PathBuf,
    svg_output: PathBuf,
    title: String,
    subtitle: Option<String>,
    width: u32,
    height: u32,
    min_value: Option<f64>,
    max_value: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PortfolioGraphOutput {
    pub html_path: String,
    pub svg_path: String,
    pub currency: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub granularity: String,
    pub point_count: usize,
}

pub async fn portfolio_graph(
    storage: Arc<dyn Storage>,
    config: &ResolvedConfig,
    options: PortfolioGraphOptions,
) -> Result<PortfolioGraphOutput> {
    let resolved = resolve_graph_options(options, config)?;
    let selection = resolve_portfolio_history_selection(
        storage.as_ref(),
        config,
        resolved.account.as_deref(),
        resolved.connection.as_deref(),
    )
    .await?;
    let history = match selection {
        PortfolioHistorySelection::Portfolio => {
            portfolio_history(
                storage,
                config,
                resolved.currency.clone(),
                resolved.start.clone(),
                resolved.end.clone(),
                resolved.granularity.clone(),
                resolved.include_prices,
            )
            .await?
        }
        PortfolioHistorySelection::Accounts(account_ids) => {
            portfolio_history_for_accounts(
                storage,
                config,
                resolved.currency.clone(),
                resolved.start.clone(),
                resolved.end.clone(),
                resolved.granularity.clone(),
                resolved.include_prices,
                account_ids,
            )
            .await?
        }
        PortfolioHistorySelection::LatentCapitalGainsTax => {
            latent_capital_gains_tax_history(
                storage,
                config,
                resolved.currency.clone(),
                resolved.start.clone(),
                resolved.end.clone(),
                resolved.granularity.clone(),
                resolved.include_prices,
            )
            .await?
        }
    };

    let svg = render_net_worth_svg(&history, &resolved)?;
    let html = render_graph_html(&resolved, &history);

    if let Some(parent) = resolved
        .svg_output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create SVG output directory {}", parent.display())
        })?;
    }
    if let Some(parent) = resolved
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create HTML output directory {}",
                parent.display()
            )
        })?;
    }

    fs::write(&resolved.svg_output, svg).with_context(|| {
        format!(
            "Failed to write SVG graph {}",
            resolved.svg_output.display()
        )
    })?;
    fs::write(&resolved.output, html)
        .with_context(|| format!("Failed to write HTML graph {}", resolved.output.display()))?;

    Ok(PortfolioGraphOutput {
        html_path: resolved.output.display().to_string(),
        svg_path: resolved.svg_output.display().to_string(),
        currency: history.currency,
        start_date: history.start_date,
        end_date: history.end_date,
        granularity: history.granularity,
        point_count: history.points.len(),
    })
}

fn resolve_graph_options(
    options: PortfolioGraphOptions,
    config: &ResolvedConfig,
) -> Result<ResolvedGraphOptions> {
    let file_options = match &options.graph_config {
        Some(path) => {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("Failed to read graph config {}", path.display()))?;
            toml::from_str::<GraphConfigFile>(&raw)
                .with_context(|| format!("Failed to parse graph config {}", path.display()))?
        }
        None => GraphConfigFile::default(),
    };

    let start = options.start.or(file_options.start);
    let end = options.end.or(file_options.end);
    let currency = options.currency.or(file_options.currency);
    let account = options.account.or(file_options.account);
    let connection = options.connection.or(file_options.connection);
    let granularity = options
        .granularity
        .or(file_options.granularity)
        .unwrap_or_else(|| config.history.portfolio_granularity.clone());
    let include_prices = options
        .include_prices
        .or(file_options.include_prices)
        .unwrap_or(config.history.include_prices);
    let title = options.title.or(file_options.title).unwrap_or_else(|| {
        if account.is_some() {
            "Keepbook Account Value".to_string()
        } else if connection.is_some() {
            "Keepbook Connection Value".to_string()
        } else {
            "Keepbook Net Worth".to_string()
        }
    });
    let subtitle = options.subtitle.or(file_options.subtitle);
    let width = options
        .width
        .or(file_options.width)
        .unwrap_or(DEFAULT_WIDTH);
    let height = options
        .height
        .or(file_options.height)
        .unwrap_or(DEFAULT_HEIGHT);
    let min_value = options.min_value.or(file_options.min_value);
    let max_value = options.max_value.or(file_options.max_value);

    if width < 360 || height < 240 {
        anyhow::bail!("Graph width and height must be at least 360x240");
    }
    if let (Some(min_value), Some(max_value)) = (min_value, max_value) {
        if min_value >= max_value {
            anyhow::bail!("Graph min-value must be less than max-value");
        }
    }

    let output = options.output.or(file_options.output).unwrap_or_else(|| {
        default_graph_output_path(
            account.as_deref(),
            connection.as_deref(),
            start.as_deref(),
            end.as_deref(),
        )
    });
    let svg_output = options
        .svg_output
        .or(file_options.svg_output)
        .unwrap_or_else(|| output.with_extension("svg"));

    Ok(ResolvedGraphOptions {
        start,
        end,
        currency,
        granularity,
        include_prices,
        account,
        connection,
        output,
        svg_output,
        title,
        subtitle,
        width,
        height,
        min_value,
        max_value,
    })
}

fn default_graph_output_path(
    account: Option<&str>,
    connection: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
) -> PathBuf {
    let prefix = if let Some(account) = account {
        format!("account-{}", file_name_token(account))
    } else if let Some(connection) = connection {
        format!("connection-{}", file_name_token(connection))
    } else {
        "net-worth".to_string()
    };
    let name = match (start, end) {
        (Some(start), Some(end)) => format!("{prefix}-{start}-to-{end}.html"),
        (Some(start), None) => format!("{prefix}-since-{start}.html"),
        (None, Some(end)) => format!("{prefix}-through-{end}.html"),
        (None, None) => format!("{prefix}.html"),
    };
    PathBuf::from("artifacts").join(name)
}

fn file_name_token(value: &str) -> String {
    let token = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if token.is_empty() {
        "selected".to_string()
    } else {
        token
    }
}

fn render_graph_html(options: &ResolvedGraphOptions, history: &HistoryOutput) -> String {
    let img_src = image_src_for_html(&options.output, &options.svg_output);
    let alt = format!(
        "{} graph with {} points",
        options.title,
        history.points.len()
    );
    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{}</title>
    <style>
      body {{ margin: 0; background: #edf2f7; display: grid; place-items: center; min-height: 100vh; }}
      img {{ width: min(96vw, {}px); height: auto; box-shadow: 0 18px 48px rgba(16,42,67,.18); border-radius: 20px; }}
    </style>
  </head>
  <body>
    <img src="{}" alt="{}" />
  </body>
</html>
"#,
        escape_html(&options.title),
        options.width,
        escape_html(&img_src),
        escape_html(&alt)
    )
}

fn image_src_for_html(html_path: &Path, svg_path: &Path) -> String {
    if html_path.parent() == svg_path.parent() {
        if let Some(file_name) = svg_path.file_name().and_then(|s| s.to_str()) {
            return format!("./{file_name}");
        }
    }
    svg_path.display().to_string()
}

fn render_net_worth_svg(history: &HistoryOutput, options: &ResolvedGraphOptions) -> Result<String> {
    let width = options.width as f64;
    let height = options.height as f64;
    let margin_left = 120.0;
    let margin_right = 56.0;
    let margin_top = 104.0;
    let margin_bottom = 96.0;
    let plot_x = margin_left;
    let plot_y = margin_top;
    let plot_w = width - margin_left - margin_right;
    let plot_h = height - margin_top - margin_bottom;

    let values: Vec<f64> = history
        .points
        .iter()
        .map(|point| {
            point
                .total_value
                .parse::<f64>()
                .with_context(|| format!("Invalid history value {}", point.total_value))
        })
        .collect::<Result<_>>()?;

    let dates: Vec<NaiveDate> = history
        .points
        .iter()
        .map(|point| {
            NaiveDate::parse_from_str(&point.date, "%Y-%m-%d")
                .with_context(|| format!("Invalid history date {}", point.date))
        })
        .collect::<Result<_>>()?;

    let (mut y_min, mut y_max) = if values.is_empty() {
        (0.0, 1.0)
    } else {
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let span = (max - min).abs();
        let pad = if span == 0.0 {
            max.abs().max(1.0) * 0.05
        } else {
            span * 0.08
        };
        (min - pad, max + pad)
    };
    if let Some(min_value) = options.min_value {
        y_min = min_value;
    }
    if let Some(max_value) = options.max_value {
        y_max = max_value;
    }
    if y_min >= y_max {
        anyhow::bail!("Graph value range is invalid after applying min/max bounds");
    }

    let min_day = dates.first().map(|d| date_days(*d)).unwrap_or(0);
    let max_day = dates.last().map(|d| date_days(*d)).unwrap_or(min_day + 1);
    let day_span = (max_day - min_day).max(1) as f64;

    let point_xy: Vec<(f64, f64)> = values
        .iter()
        .zip(dates.iter())
        .map(|(value, date)| {
            let x = if values.len() == 1 {
                plot_x + plot_w / 2.0
            } else {
                plot_x + ((date_days(*date) - min_day) as f64 / day_span) * plot_w
            };
            let y = plot_y + ((y_max - value) / (y_max - y_min)) * plot_h;
            (x, y)
        })
        .collect();

    let line_path = path_from_points(&point_xy);
    let area_path = area_path_from_points(&point_xy, plot_y + plot_h);
    let start_label = history
        .points
        .first()
        .map(|p| p.date.as_str())
        .unwrap_or("no data");
    let end_label = history
        .points
        .last()
        .map(|p| p.date.as_str())
        .unwrap_or("no data");
    let subtitle = options
        .subtitle
        .clone()
        .unwrap_or_else(|| format!("{start_label} to {end_label} - {}", history.currency));

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" role="img" aria-labelledby="title desc">
  <title id="title">{}</title>
  <desc id="desc">Net worth graph from {} to {} in {}</desc>
  <rect width="100%" height="100%" fill="#f8fafc"/>
  <text x="{}" y="54" font-size="34" fill="#102a43" font-family="ui-sans-serif, system-ui, sans-serif">{}</text>
  <text x="{}" y="84" font-size="18" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">{}</text>
  <rect x="{}" y="{}" width="{}" height="{}" fill="#ffffff" stroke="#d9e2ec" rx="8"/>
"##,
        options.width,
        options.height,
        options.width,
        options.height,
        escape_html(&options.title),
        escape_html(start_label),
        escape_html(end_label),
        escape_html(&history.currency),
        margin_left,
        escape_html(&options.title),
        margin_left,
        escape_html(&subtitle),
        plot_x,
        plot_y,
        plot_w,
        plot_h
    ));

    for i in 0..=5 {
        let ratio = i as f64 / 5.0;
        let y = plot_y + ratio * plot_h;
        let value = y_max - ratio * (y_max - y_min);
        svg.push_str(&format!(
            r##"  <line x1="{}" y1="{:.2}" x2="{}" y2="{:.2}" stroke="#eef2f7"/>
  <text x="{}" y="{:.2}" font-size="14" text-anchor="end" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">{}</text>
"##,
            plot_x,
            y,
            plot_x + plot_w,
            y,
            plot_x - 14.0,
            y + 5.0,
            escape_html(&format_currency_tick(value, &history.currency))
        ));
    }

    for (date, x) in x_ticks(&dates, plot_x, plot_w) {
        svg.push_str(&format!(
            r##"  <line x1="{:.2}" y1="{}" x2="{:.2}" y2="{}" stroke="#e5eaf1"/>
  <text x="{:.2}" y="{}" font-size="14" text-anchor="middle" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">{}</text>
"##,
            x,
            plot_y,
            x,
            plot_y + plot_h + 8.0,
            x,
            plot_y + plot_h + 34.0,
            escape_html(&date.format("%Y-%m-%d").to_string())
        ));
    }

    if !area_path.is_empty() {
        svg.push_str(&format!(
            r##"  <path d="{}" fill="#2f80ed" opacity="0.14"/>
  <path d="{}" fill="none" stroke="#1c7ed6" stroke-width="4" stroke-linejoin="round" stroke-linecap="round"/>
"##,
            area_path, line_path
        ));
    } else {
        svg.push_str(&format!(
            r##"  <text x="{}" y="{}" font-size="24" text-anchor="middle" fill="#627d98" font-family="ui-sans-serif, system-ui, sans-serif">No history points</text>
"##,
            plot_x + plot_w / 2.0,
            plot_y + plot_h / 2.0
        ));
    }

    if let Some((x, y)) = point_xy.last() {
        if let Some(value) = values.last() {
            svg.push_str(&format!(
                r##"  <circle cx="{:.2}" cy="{:.2}" r="6" fill="#0b7285" stroke="#ffffff" stroke-width="3"/>
  <text x="{:.2}" y="{:.2}" font-size="18" fill="#102a43" font-family="ui-sans-serif, system-ui, sans-serif">{}</text>
"##,
                x,
                y,
                x + 12.0,
                y - 12.0,
                escape_html(&format_currency_tick(*value, &history.currency))
            ));
        }
    }

    svg.push_str(&format!(
        r##"  <text x="{}" y="{}" font-size="16" fill="#829ab1" font-family="ui-sans-serif, system-ui, sans-serif">Source: keepbook portfolio graph --start {} --end {} --granularity {}</text>
</svg>
"##,
        margin_left,
        height - 28.0,
        history.start_date.as_deref().unwrap_or("earliest"),
        history.end_date.as_deref().unwrap_or("today"),
        escape_html(&history.granularity)
    ));

    Ok(svg)
}

fn date_days(date: NaiveDate) -> i32 {
    date.signed_duration_since(NaiveDate::from_ymd_opt(1970, 1, 1).unwrap())
        .num_days() as i32
}

fn path_from_points(points: &[(f64, f64)]) -> String {
    points
        .iter()
        .enumerate()
        .map(|(idx, (x, y))| {
            if idx == 0 {
                format!("M {:.2} {:.2}", x, y)
            } else {
                format!("L {:.2} {:.2}", x, y)
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn area_path_from_points(points: &[(f64, f64)], baseline_y: f64) -> String {
    let Some((first_x, _)) = points.first() else {
        return String::new();
    };
    let Some((last_x, _)) = points.last() else {
        return String::new();
    };
    format!(
        "{} L {:.2} {:.2} L {:.2} {:.2} Z",
        path_from_points(points),
        last_x,
        baseline_y,
        first_x,
        baseline_y
    )
}

fn x_ticks(dates: &[NaiveDate], plot_x: f64, plot_w: f64) -> Vec<(NaiveDate, f64)> {
    if dates.is_empty() {
        return Vec::new();
    }
    let count = dates.len().min(6);
    if count == 1 {
        return vec![(dates[0], plot_x + plot_w / 2.0)];
    }

    let mut ticks = Vec::with_capacity(count);
    for i in 0..count {
        let idx = ((i as f64 * (dates.len() - 1) as f64) / (count - 1) as f64).round() as usize;
        let x = plot_x + (i as f64 / (count - 1) as f64) * plot_w;
        let date = dates[idx];
        if ticks.last().map(|(last, _)| *last != date).unwrap_or(true) {
            ticks.push((date, x));
        }
    }
    ticks
}

fn format_currency_tick(value: f64, currency: &str) -> String {
    let sign = if value < 0.0 { "-" } else { "" };
    let value = value.abs();
    let compact = if value >= 1_000_000_000.0 {
        format!("{:.1}B", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("{:.1}M", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1}K", value / 1_000.0)
    } else {
        format!("{:.0}", value)
    };
    format!("{sign}{compact} {currency}")
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::types::HistoryPoint;

    #[test]
    fn default_output_path_uses_range() {
        assert_eq!(
            default_graph_output_path(None, None, Some("2026-03-01"), Some("2026-04-01")),
            PathBuf::from("artifacts/net-worth-2026-03-01-to-2026-04-01.html")
        );
    }

    #[test]
    fn account_default_output_path_uses_scope() {
        assert_eq!(
            default_graph_output_path(
                Some("Cash Account"),
                None,
                Some("2026-03-01"),
                Some("2026-04-01")
            ),
            PathBuf::from("artifacts/account-cash-account-2026-03-01-to-2026-04-01.html")
        );
    }

    #[test]
    fn renders_svg_for_history_points() -> Result<()> {
        let history = HistoryOutput {
            currency: "USD".to_string(),
            start_date: Some("2026-03-01".to_string()),
            end_date: Some("2026-03-03".to_string()),
            granularity: "daily".to_string(),
            points: vec![
                HistoryPoint {
                    timestamp: "2026-03-01T00:00:00+00:00".to_string(),
                    date: "2026-03-01".to_string(),
                    total_value: "100".to_string(),
                    prospective_capital_gains_tax: None,
                    percentage_change_from_previous: None,
                    change_triggers: None,
                },
                HistoryPoint {
                    timestamp: "2026-03-03T00:00:00+00:00".to_string(),
                    date: "2026-03-03".to_string(),
                    total_value: "125".to_string(),
                    prospective_capital_gains_tax: None,
                    percentage_change_from_previous: Some("25.00".to_string()),
                    change_triggers: None,
                },
            ],
            summary: None,
        };
        let options = ResolvedGraphOptions {
            start: Some("2026-03-01".to_string()),
            end: Some("2026-03-03".to_string()),
            currency: None,
            granularity: "daily".to_string(),
            include_prices: true,
            account: None,
            connection: None,
            output: PathBuf::from("graph.html"),
            svg_output: PathBuf::from("graph.svg"),
            title: "Test Graph".to_string(),
            subtitle: None,
            width: 800,
            height: 500,
            min_value: None,
            max_value: None,
        };

        let svg = render_net_worth_svg(&history, &options)?;

        assert!(svg.contains("<path d=\"M "));
        assert!(svg.contains("Test Graph"));
        assert!(svg.contains("125 USD"));
        Ok(())
    }
}
