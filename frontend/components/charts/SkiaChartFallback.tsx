import React from "react";
import { ActivityIndicator, StyleSheet } from "react-native";

import { Text, View } from "@/components/Themed";
import { chartDefaults } from "./chart-colors";

interface SkiaChartFallbackProps {
  height?: number;
}

export function SkiaChartFallback({
  height = chartDefaults.height,
}: SkiaChartFallbackProps) {
  return (
    <View style={[styles.container, { height }]}>
      <ActivityIndicator size="small" color="#2f95dc" />
      <Text style={styles.text}>Loading chart</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    justifyContent: "center",
    alignItems: "center",
    gap: 8,
  },
  text: {
    color: "rgba(255, 255, 255, 0.5)",
    fontSize: 13,
  },
});
