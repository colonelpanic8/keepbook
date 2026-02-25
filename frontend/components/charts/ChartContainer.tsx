import React, { ReactNode } from "react";
import { ActivityIndicator, StyleSheet } from "react-native";

import { Text, View } from "@/components/Themed";
import { chartDefaults } from "./chart-colors";

interface ChartContainerProps {
  loading: boolean;
  error: string | null;
  height?: number;
  children: ReactNode;
}

export function ChartContainer({
  loading,
  error,
  height = chartDefaults.height,
  children,
}: ChartContainerProps) {
  if (loading) {
    return (
      <View style={[styles.center, { height }]}>
        <ActivityIndicator size="large" color="#2f95dc" />
      </View>
    );
  }

  if (error) {
    return (
      <View style={[styles.center, { height }]}>
        <Text style={styles.errorText}>{error}</Text>
      </View>
    );
  }

  return <>{children}</>;
}

const styles = StyleSheet.create({
  center: {
    justifyContent: "center",
    alignItems: "center",
  },
  errorText: {
    color: "#e74c3c",
    fontSize: 14,
    textAlign: "center",
    paddingHorizontal: 16,
  },
});
