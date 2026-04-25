import React from "react";
import { WithSkiaWeb } from "@shopify/react-native-skia/lib/module/web";

import { SkiaChartFallback } from "./SkiaChartFallback";
import type {
  SpendingChartProps,
  SpendingDataPoint,
} from "./SpendingChart.types";

export type { SpendingChartProps, SpendingDataPoint };

export function SpendingChart(props: SpendingChartProps) {
  return (
    <WithSkiaWeb<SpendingChartProps>
      getComponent={() => import("./SpendingChart.skia")}
      componentProps={props}
      fallback={<SkiaChartFallback height={props.height} />}
    />
  );
}

export default SpendingChart;
