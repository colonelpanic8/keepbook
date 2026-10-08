export interface MetricCardProps {
  label: string;
  /** Preformatted headline value, e.g. "$6,814.24". */
  value: string;
  /** One line of context, e.g. the as-of date. */
  detail: string;
}

/** A headline number with its label and one line of context. Use three in a `SummaryGrid`. Mirrors `MetricCard` in `views/shared.rs`. */
export function MetricCard({ label, value, detail }: MetricCardProps) {
  return (
    <article className="metric">
      <span className="metric-label">{label}</span>
      <strong>{value}</strong>
      <small>{detail}</small>
    </article>
  );
}
