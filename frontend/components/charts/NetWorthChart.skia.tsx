import React from "react";
import { Platform, StyleSheet } from "react-native";
import {
  CartesianChart,
  Line,
  Area,
  useChartPressState,
  type ChartBounds,
  type PointsArray,
} from "victory-native";
import {
  Circle,
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
import type { NetWorthChartProps } from "./NetWorthChart.types";

// eslint-disable-next-line @typescript-eslint/no-var-requires
const spaceMono = require("../../assets/fonts/SpaceMono-Regular.ttf");

type ChartDatum = { x: string; value: number };
type ChartScale = (value: number) => number;

function formatDate(dateStr: string): string {
  const months = [
    "Jan",
    "Feb",
    "Mar",
    "Apr",
    "May",
    "Jun",
    "Jul",
    "Aug",
    "Sep",
    "Oct",
    "Nov",
    "Dec",
  ];
  const [, m, d] = dateStr.split("-");
  const monthIdx = parseInt(m, 10) - 1;
  const day = parseInt(d, 10);
  if (monthIdx < 0 || monthIdx >= months.length || !Number.isFinite(day)) {
    return dateStr;
  }
  return `${months[monthIdx]} ${day}`;
}

function formatAxisDate(dateStr: string, showYear: boolean): string {
  if (!showYear) return formatDate(dateStr);

  const [year, month] = dateStr.split("-");
  const monthIdx = parseInt(month, 10) - 1;
  if (!year || monthIdx < 0 || monthIdx >= 12) return dateStr;
  return `${formatDate(`${year}-${month}-01`).split(" ")[0]} '${year.slice(2)}`;
}

function formatFullDate(dateStr: string): string {
  const [year, month, day] = dateStr.split("-");
  if (!year || !month || !day) return dateStr;
  return `${formatDate(dateStr)}, ${year}`;
}

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

function formatTooltipDollarValue(value: number): string {
  return `$${value.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function NetWorthChart({
  data,
  height = chartDefaults.height,
  yDomain,
}: NetWorthChartProps) {
  const font = useFont(spaceMono, 11);
  const colorScheme = useColorScheme();
  const axisColors = React.useMemo(
    () => chartAxisColors(colorScheme === "dark"),
    [colorScheme],
  );
  const [chartBounds, setChartBounds] = React.useState<ChartBounds | null>(
    null,
  );
  const [cursorIndex, setCursorIndex] = React.useState<number | null>(null);

  const { state, isActive } = useChartPressState({
    x: "" as string,
    y: { value: 0 },
  });

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
    x: d.date,
    value: d.value,
  }));
  const firstDateMs = Date.parse(`${chartData[0].x}T00:00:00.000Z`);
  const lastDateMs = Date.parse(`${chartData[chartData.length - 1].x}T00:00:00.000Z`);
  const showYearOnXAxis =
    Number.isFinite(firstDateMs) &&
    Number.isFinite(lastDateMs) &&
    lastDateMs - firstDateMs > 370 * 86400000;
  const measuredChartWidth =
    chartBounds !== null ? chartBounds.right - chartBounds.left : undefined;
  const measuredChartHeight =
    chartBounds !== null ? chartBounds.bottom - chartBounds.top : undefined;
  const xTickValues = evenlySpacedIndices(
    chartData.length,
    tickCountForSpan(
      measuredChartWidth,
      showYearOnXAxis ? 132 : 112,
      chartData.length === 1 ? 1 : 2,
      6,
    ),
  );
  const yTickCount = tickCountForSpan(measuredChartHeight, 68, 3, 6);

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
            if (!isActive) setCursorIndex(null);
          },
        }
      : {};

  return (
    <View style={{ height, position: "relative" }} {...pointerHandlers}>
      <CartesianChart
        data={chartData}
        xKey="x"
        yKeys={["value"]}
        domain={yDomain ? { y: yDomain } : undefined}
        domainPadding={chartDefaults.domainPadding}
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
          formatXLabel: (label) => formatAxisDate(String(label), showYearOnXAxis),
        }}
        yAxis={[{
          yKeys: ["value"],
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
            points={points.value}
            chartBounds={chartBounds}
            xTicks={xTickValues}
            yTicks={yTicks}
            xScale={xScale}
            yScale={yScale}
            tickColor={axisColors.tick}
            cursorIndex={cursorIndex}
            data={chartData}
            font={font}
          />
        )}
      </CartesianChart>
    </View>
  );
}

export default NetWorthChart;

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
}: ChartContentProps) {
  const activePoint =
    cursorIndex !== null && cursorIndex >= 0 && cursorIndex < points.length
      ? points[cursorIndex]
      : undefined;
  const activeDatum =
    cursorIndex !== null && cursorIndex >= 0 && cursorIndex < data.length
      ? data[cursorIndex]
      : undefined;
  const activeY =
    activePoint?.y !== undefined && activePoint.y !== null
      ? activePoint.y
      : undefined;

  const tooltipWidth = 156;
  const tooltipHeight = 42;
  const tooltipX =
    activePoint && activePoint.x + tooltipWidth + 12 > chartBounds.right
      ? Math.max(chartBounds.left + 6, activePoint.x - tooltipWidth - 10)
      : Math.min(chartBounds.right - tooltipWidth - 6, (activePoint?.x ?? chartBounds.left) + 10);
  const tooltipY = chartBounds.top + 8;

  return (
    <>
      <Area
        points={points}
        y0={chartBounds.bottom}
        color={chartColors.accentFill}
        animate={{ type: "timing", duration: 500 }}
      />
      <Line
        points={points}
        color={chartColors.accent}
        strokeWidth={2}
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
      {activePoint && activeY !== undefined && activeDatum && (
        <>
          <SkiaLine
            p1={{ x: activePoint.x, y: chartBounds.top }}
            p2={{ x: activePoint.x, y: chartBounds.bottom }}
            color="rgba(47, 149, 220, 0.55)"
            strokeWidth={1}
          />
          <Circle
            cx={activePoint.x}
            cy={activeY}
            r={7}
            color={chartColors.accentDim}
          />
          <Circle
            cx={activePoint.x}
            cy={activeY}
            r={4}
            color={chartColors.accent}
          />
          <RoundedRect
            x={tooltipX}
            y={tooltipY}
            width={tooltipWidth}
            height={tooltipHeight}
            r={6}
            color="rgba(17, 24, 39, 0.88)"
          />
          <SkiaText
            x={tooltipX + 10}
            y={tooltipY + 16}
            text={formatFullDate(activeDatum.x)}
            font={font}
            color="#fff"
          />
          <SkiaText
            x={tooltipX + 10}
            y={tooltipY + 32}
            text={formatTooltipDollarValue(activeDatum.value)}
            font={font}
            color="#fff"
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
});
