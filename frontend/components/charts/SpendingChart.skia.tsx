import React from "react";
import { Platform, StyleSheet } from "react-native";
import {
  CartesianChart,
  Bar,
  useChartPressState,
  type ChartBounds,
  type PointsArray,
} from "victory-native";
import {
  Group,
  Line as SkiaLine,
  RoundedRect,
  Text as SkiaText,
  useFont,
} from "@shopify/react-native-skia";
import { runOnJS, useAnimatedReaction } from "react-native-reanimated";

import { Text, View } from "@/components/Themed";
import { useColorScheme } from "@/components/useColorScheme";
import { chartAxisColors, chartColors, chartDefaults } from "./chart-colors";
import { evenlySpacedIndices, tickCountForSpan } from "./chart-ticks";
import type { SpendingChartProps } from "./SpendingChart.types";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const spaceMono = require("../../assets/fonts/SpaceMono-Regular.ttf");

type ChartDatum = {
  x: string;
  total: number;
  startDate?: string;
  endDate?: string;
  transactionCount?: number;
};
type ChartScale = (value: number) => number;

function formatDollarValue(value: number): string {
  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  if (abs >= 1_000_000) {
    const m = abs / 1_000_000;
    return `${sign}$${m.toFixed(1)}M`;
  }
  if (abs >= 1_000) {
    const k = abs / 1_000;
    return `${sign}$${k.toFixed(0)}K`;
  }
  return `${sign}$${abs.toFixed(0)}`;
}

function formatTooltipDollarValue(value: number, currency: string): string {
  const symbol = currency === "USD" ? "$" : `${currency} `;
  return `${symbol}${value.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

function formatFullDate(dateStr: string | undefined): string {
  if (!dateStr) return "";
  const [year, month, day] = dateStr.split("-");
  if (!year || !month || !day) return dateStr;
  const date = new Date(`${dateStr}T00:00:00.000Z`);
  if (Number.isNaN(date.getTime())) return dateStr;
  return date.toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
    timeZone: "UTC",
  });
}

function formatRangeLabel(datum: ChartDatum): string {
  if (!datum.startDate && !datum.endDate) return datum.x;
  if (datum.startDate === datum.endDate || !datum.endDate) {
    return formatFullDate(datum.startDate);
  }
  return `${formatFullDate(datum.startDate)} - ${formatFullDate(datum.endDate)}`;
}

function formatTransactionCount(count: number | undefined): string {
  const n = count ?? 0;
  return `${n} txn${n === 1 ? "" : "s"}`;
}

export function SpendingChart({
  data,
  height = chartDefaults.height,
  currency = "USD",
  periodLabel = "period",
}: SpendingChartProps) {
  const font = useFont(spaceMono, 11);
  const colorScheme = useColorScheme();
  const axisColors = React.useMemo(
    () => chartAxisColors(colorScheme === "dark"),
    [colorScheme],
  );
  const overlayLabelColor =
    colorScheme === "dark"
      ? "rgba(255, 255, 255, 0.52)"
      : "rgba(17, 24, 39, 0.58)";

  const { state } = useChartPressState({
    x: "" as string,
    y: { total: 0 },
  });
  const [chartBounds, setChartBounds] = React.useState<ChartBounds | null>(
    null,
  );
  const [cursorIndex, setCursorIndex] = React.useState<number | null>(null);

  const setPressCursorIndex = React.useCallback((index: number) => {
    setCursorIndex(index >= 0 ? index : null);
  }, []);

  useAnimatedReaction(
    () => ({
      active: state.isActive.value,
      index: state.matchedIndex.value,
    }),
    (cursor) => {
      runOnJS(setPressCursorIndex)(cursor.active ? cursor.index : -1);
    },
    [setPressCursorIndex],
  );

  if (data.length === 0) {
    return (
      <View style={[styles.empty, { height }]}>
        <Text style={styles.emptyText}>No data available</Text>
      </View>
    );
  }

  const chartData: ChartDatum[] = data.map((d) => ({
    x: d.label,
    total: d.total,
    startDate: d.startDate,
    endDate: d.endDate,
    transactionCount: d.transactionCount,
  }));

  const measuredChartWidth =
    chartBounds !== null ? chartBounds.right - chartBounds.left : undefined;
  const measuredChartHeight =
    chartBounds !== null ? chartBounds.bottom - chartBounds.top : undefined;
  const xTickValues = evenlySpacedIndices(
    chartData.length,
    tickCountForSpan(
      measuredChartWidth,
      108,
      chartData.length === 1 ? 1 : 2,
      7,
    ),
  );
  const yTickCount = tickCountForSpan(measuredChartHeight, 68, 3, 6);
  const maxTotal = Math.max(...chartData.map((d) => d.total));

  const setHoverCursorIndex = (relativeX: number) => {
    if (!chartBounds || chartData.length === 0) return;
    if (relativeX < chartBounds.left || relativeX > chartBounds.right) {
      setCursorIndex(null);
      return;
    }

    const ratio =
      (relativeX - chartBounds.left) / (chartBounds.right - chartBounds.left);
    const nextIndex = Math.round(ratio * (chartData.length - 1));
    setCursorIndex(Math.max(0, Math.min(chartData.length - 1, nextIndex)));
  };

  const pointerHandlers =
    Platform.OS === "web"
      ? {
          onPointerMove: (event: any) => {
            const rect = event.currentTarget.getBoundingClientRect();
            setHoverCursorIndex(event.clientX - rect.left);
          },
          onPointerLeave: () => {
            if (!state.isActive.value) setCursorIndex(null);
          },
        }
      : {};

  return (
    <View style={{ height, position: "relative" }} {...pointerHandlers}>
      <CartesianChart
        data={chartData}
        xKey="x"
        yKeys={["total"]}
        domain={{ y: [0, maxTotal > 0 ? maxTotal * 1.12 : 1] }}
        domainPadding={{ ...chartDefaults.domainPadding, left: 20, right: 20 }}
        chartPressState={state}
        onChartBoundsChange={setChartBounds}
        xAxis={{
          font,
          tickCount: xTickValues.length,
          tickValues: xTickValues,
          lineColor: axisColors.gridLine,
          lineWidth: 1,
          labelColor: axisColors.labelStrong,
          labelOffset: 4,
          formatXLabel: (label) => String(label),
        }}
        yAxis={[{
          yKeys: ["total"],
          font,
          tickCount: yTickCount,
          lineColor: axisColors.gridLine,
          lineWidth: 1,
          labelColor: axisColors.labelStrong,
          labelOffset: 4,
          formatYLabel: (label) => formatDollarValue(Number(label)),
        }]}
        frame={{
          lineColor: axisColors.gridLine,
          lineWidth: 1,
        }}
      >
        {({ points, chartBounds, yTicks, xScale, yScale }) => (
          <ChartContent
            points={points.total}
            chartBounds={chartBounds}
            xTicks={xTickValues}
            yTicks={yTicks}
            xScale={xScale}
            yScale={yScale}
            tickColor={axisColors.tick}
            cursorIndex={cursorIndex}
            data={chartData}
            font={font}
            currency={currency}
          />
        )}
      </CartesianChart>
      <View pointerEvents="none" style={styles.yAxisLabel}>
        <Text style={[styles.axisLabelText, { color: overlayLabelColor }]}>
          Amount ({currency})
        </Text>
      </View>
      <Text style={[styles.xAxisLabel, { color: overlayLabelColor }]}>
        {periodLabel} start
      </Text>
    </View>
  );
}

export default SpendingChart;

interface ChartContentProps {
  points: PointsArray;
  chartBounds: ChartBounds;
  xTicks: number[];
  yTicks: number[];
  xScale: ChartScale;
  yScale: ChartScale;
  tickColor: string;
  cursorIndex: number | null;
  data: ChartDatum[];
  font: ReturnType<typeof useFont>;
  currency: string;
}

function ChartContent({
  points,
  chartBounds,
  xTicks,
  yTicks,
  xScale,
  yScale,
  tickColor,
  cursorIndex,
  data,
  font,
  currency,
}: ChartContentProps) {
  const activePoint =
    cursorIndex !== null && cursorIndex >= 0 && cursorIndex < points.length
      ? points[cursorIndex]
      : undefined;
  const activeDatum =
    cursorIndex !== null && cursorIndex >= 0 && cursorIndex < data.length
      ? data[cursorIndex]
      : undefined;

  const tooltipWidth = 182;
  const tooltipHeight = 58;
  const tooltipX =
    activePoint && activePoint.x + tooltipWidth + 12 > chartBounds.right
      ? Math.max(chartBounds.left + 6, activePoint.x - tooltipWidth - 10)
      : Math.min(
          chartBounds.right - tooltipWidth - 6,
          (activePoint?.x ?? chartBounds.left) + 10,
        );
  const tooltipY = chartBounds.top + 8;

  return (
    <>
      <Bar
        points={points}
        chartBounds={chartBounds}
        color={chartColors.bar}
        roundedCorners={{ topLeft: 4, topRight: 4 }}
        animate={{ type: "timing", duration: 500 }}
      />
      <ChartTickMarks
        chartBounds={chartBounds}
        xTicks={xTicks}
        yTicks={yTicks}
        xScale={xScale}
        yScale={yScale}
        tickColor={tickColor}
      />
      {activePoint && activeDatum && (
        <>
          <SkiaLine
            p1={{ x: activePoint.x, y: chartBounds.top }}
            p2={{ x: activePoint.x, y: chartBounds.bottom }}
            color="rgba(47, 149, 220, 0.55)"
            strokeWidth={1}
          />
          <RoundedRect
            x={tooltipX}
            y={tooltipY}
            width={tooltipWidth}
            height={tooltipHeight}
            r={6}
            color="rgba(17, 24, 39, 0.9)"
          />
          <SkiaText
            x={tooltipX + 10}
            y={tooltipY + 16}
            text={formatRangeLabel(activeDatum)}
            font={font}
            color="#fff"
          />
          <SkiaText
            x={tooltipX + 10}
            y={tooltipY + 34}
            text={formatTooltipDollarValue(activeDatum.total, currency)}
            font={font}
            color="#fff"
          />
          <SkiaText
            x={tooltipX + 10}
            y={tooltipY + 50}
            text={formatTransactionCount(activeDatum.transactionCount)}
            font={font}
            color="#d1d5db"
          />
        </>
      )}
    </>
  );
}

interface ChartTickMarksProps {
  chartBounds: ChartBounds;
  xTicks: number[];
  yTicks: number[];
  xScale: ChartScale;
  yScale: ChartScale;
  tickColor: string;
}

function ChartTickMarks({
  chartBounds,
  xTicks,
  yTicks,
  xScale,
  yScale,
  tickColor,
}: ChartTickMarksProps) {
  const tickLength = 7;

  return (
    <Group>
      {xTicks.map((tick) => {
        const x = xScale(tick);
        if (x < chartBounds.left || x > chartBounds.right) return null;
        return (
          <SkiaLine
            key={`x-tick-${tick}`}
            p1={{ x, y: chartBounds.bottom - tickLength }}
            p2={{ x, y: chartBounds.bottom }}
            color={tickColor}
            strokeWidth={1.5}
          />
        );
      })}
      {yTicks.map((tick) => {
        const y = yScale(tick);
        if (y < chartBounds.top || y > chartBounds.bottom) return null;
        return (
          <SkiaLine
            key={`y-tick-${tick}`}
            p1={{ x: chartBounds.left, y }}
            p2={{ x: chartBounds.left + tickLength, y }}
            color={tickColor}
            strokeWidth={1.5}
          />
        );
      })}
    </Group>
  );
}

const styles = StyleSheet.create({
  empty: {
    justifyContent: "center",
    alignItems: "center",
  },
  emptyText: {
    color: "rgba(255, 255, 255, 0.5)",
    fontSize: 14,
  },
  yAxisLabel: {
    position: "absolute",
    left: 2,
    top: 8,
    backgroundColor: "transparent",
  },
  axisLabelText: {
    fontSize: 11,
    fontWeight: "600",
  },
  xAxisLabel: {
    position: "absolute",
    right: 12,
    bottom: 0,
    fontSize: 11,
    fontWeight: "600",
  },
});
