export interface NetWorthDataPoint {
  date: string; // YYYY-MM-DD
  value: number; // portfolio total value
}

export interface NetWorthChartProps {
  data: NetWorthDataPoint[];
  height?: number;
  yDomain?: [number, number];
}
