export interface SpendingDataPoint {
  label: string; // period label, e.g. Jan 1
  total: number; // spending total
  startDate?: string;
  endDate?: string;
  transactionCount?: number;
}

export interface SpendingChartProps {
  data: SpendingDataPoint[];
  height?: number;
  currency?: string;
  periodLabel?: string;
}
