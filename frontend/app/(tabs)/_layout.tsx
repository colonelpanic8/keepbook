import React from 'react';
import FontAwesome from '@expo/vector-icons/FontAwesome';
import { Tabs } from 'expo-router';
import Colors from '@/constants/Colors';
import { useColorScheme } from '@/components/useColorScheme';
import { useClientOnlyValue } from '@/components/useClientOnlyValue';

function TabBarIcon(props: { name: React.ComponentProps<typeof FontAwesome>['name']; color: string }) {
  return <FontAwesome size={24} style={{ marginBottom: -3 }} {...props} />;
}

export default function TabLayout() {
  const colorScheme = useColorScheme();
  return (
    <Tabs screenOptions={{
      tabBarActiveTintColor: Colors[colorScheme ?? 'light'].tint,
      headerShown: useClientOnlyValue(false, true),
    }}>
      <Tabs.Screen name="index" options={{ title: 'Net Worth', tabBarIcon: ({ color }) => <TabBarIcon name="line-chart" color={color} /> }} />
      <Tabs.Screen name="spending" options={{ title: 'Spending', tabBarIcon: ({ color }) => <TabBarIcon name="bar-chart" color={color} /> }} />
      <Tabs.Screen name="accounts" options={{ title: 'Accounts', tabBarIcon: ({ color }) => <TabBarIcon name="bank" color={color} /> }} />
      <Tabs.Screen name="settings" options={{ title: 'Settings', tabBarIcon: ({ color }) => <TabBarIcon name="cog" color={color} /> }} />
    </Tabs>
  );
}
