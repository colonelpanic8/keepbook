import React from "react";
import { WithSkiaWeb } from "@shopify/react-native-skia/lib/module/web";

import { SkiaChartFallback } from "./SkiaChartFallback";
import type {
  NetWorthChartProps,
  NetWorthDataPoint,
} from "./NetWorthChart.types";

export type { NetWorthChartProps, NetWorthDataPoint };

export function NetWorthChart(props: NetWorthChartProps) {
  return (
    <WithSkiaWeb<NetWorthChartProps>
      getComponent={() => import("./NetWorthChart.skia")}
      componentProps={props}
      fallback={<SkiaChartFallback height={props.height} />}
    />
  );
}

export default NetWorthChart;
