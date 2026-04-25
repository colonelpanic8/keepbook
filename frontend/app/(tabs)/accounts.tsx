import React, { useCallback, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useFocusEffect } from 'expo-router';
import { Text, View } from '@/components/Themed';
import KeepbookNative from '@/modules/keepbook-native';

type AssetSummary =
  | { type: 'currency'; iso_code: string }
  | { type: 'equity'; ticker: string; exchange?: string }
  | { type: 'crypto'; symbol: string; network?: string };
type AssetBalanceSummary = {
  asset: AssetSummary;
  amount: string;
  cost_basis?: string;
  value_in_base: string | null;
  base_currency: string;
};
type BalanceSnapshotSummary = {
  timestamp: string;
  balances: AssetBalanceSummary[];
  total_value_in_base?: string | null;
  base_currency?: string;
  currency_decimals?: number;
};
type AccountSummary = {
  id: string;
  name: string;
  connection_id: string;
  created_at: string;
  active: boolean;
  current_balance?: BalanceSnapshotSummary;
};
type ConnectionSummary = { id: string; name: string; synchronizer: string; status: string };

function assetLabel(asset: AssetSummary): string {
  switch (asset.type) {
    case 'currency':
      return asset.iso_code;
    case 'equity':
      return asset.exchange ? `${asset.ticker}.${asset.exchange}` : asset.ticker;
    case 'crypto':
      return asset.symbol;
  }
}

function formatAmount(amount: string): string {
  const parsed = Number(amount);
  if (!Number.isFinite(parsed)) return amount;
  return parsed.toLocaleString('en-US', {
    maximumFractionDigits: 8,
  });
}

function formatBalance(balance: AssetBalanceSummary, currencyDecimals: number | undefined): string {
  const formattedAmount =
    balance.asset.type === 'currency'
      ? formatCurrencyAmount(balance.amount, currencyDecimals)
      : formatAmount(balance.amount);
  return `${formattedAmount} ${assetLabel(balance.asset)}`;
}

function formatBaseBalance(balance: AssetBalanceSummary, currencyDecimals: number | undefined): string {
  if (balance.value_in_base === null) return `Base value unavailable`;
  return `${formatCurrencyAmount(balance.value_in_base, currencyDecimals)} ${balance.base_currency}`;
}

function formatCurrencyAmount(amount: string, currencyDecimals: number | undefined): string {
  const parsed = Number(amount);
  if (!Number.isFinite(parsed)) return amount;
  const decimals = currencyDecimals ?? 2;
  return parsed.toLocaleString('en-US', {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

function formatAccountTotal(balance: BalanceSnapshotSummary): string {
  if (balance.total_value_in_base === null) return 'Total unavailable';
  if (balance.total_value_in_base === undefined || balance.base_currency === undefined) return '';
  return `${formatCurrencyAmount(balance.total_value_in_base, balance.currency_decimals)} ${balance.base_currency}`;
}

function formatTimestamp(timestamp: string): string {
  const parsed = new Date(timestamp);
  if (Number.isNaN(parsed.getTime())) return timestamp;
  return parsed.toLocaleDateString('en-US', { month: 'short', day: 'numeric', year: 'numeric' });
}

export default function AccountsScreen() {
  const [accounts, setAccounts] = useState<AccountSummary[]>([]);
  const [connections, setConnections] = useState<ConnectionSummary[]>([]);

  const refresh = useCallback(async () => {
    const saved = await AsyncStorage.getItem('keepbook.data_dir');
    const dataDir = saved || KeepbookNative.demoDataDir();
    const [acctJson, connJson] = await Promise.all([
      KeepbookNative.listAccounts(dataDir),
      KeepbookNative.listConnections(dataDir),
    ]);
    try { setAccounts(JSON.parse(acctJson)); } catch { setAccounts([]); }
    try { setConnections(JSON.parse(connJson)); } catch { setConnections([]); }
  }, []);

  useFocusEffect(useCallback(() => { void refresh(); }, [refresh]));

  const connMap = new Map(connections.map((c) => [c.id, c]));
  const grouped = new Map<string, { connection: ConnectionSummary | null; accounts: AccountSummary[] }>();
  for (const acct of accounts) {
    if (!grouped.has(acct.connection_id)) {
      grouped.set(acct.connection_id, { connection: connMap.get(acct.connection_id) ?? null, accounts: [] });
    }
    grouped.get(acct.connection_id)!.accounts.push(acct);
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={styles.content}>
      {grouped.size === 0 && <Text style={styles.muted}>No accounts. Sync data from Settings.</Text>}
      {[...grouped.entries()].map(([connId, { connection, accounts: accts }]) => (
        <View key={connId} style={styles.group}>
          <Text style={styles.groupTitle}>{connection?.name ?? connId}</Text>
          {accts.map((a) => (
            <View key={a.id} style={styles.card}>
              <View style={styles.cardHeader}>
                <Text style={styles.cardTitle}>{a.name}{a.active ? '' : ' (inactive)'}</Text>
                <View style={styles.balanceBlock}>
                  {a.current_balance && a.current_balance.balances.length > 0 ? (
                    a.current_balance.balances.map((balance, index) => (
                      <View key={`${assetLabel(balance.asset)}-${index}`} style={styles.balanceItem}>
                        <Text style={styles.balanceText}>
                          {formatBalance(balance, a.current_balance?.currency_decimals)}
                        </Text>
                        <Text
                          lightColor="#667085"
                          darkColor="rgba(255,255,255,0.62)"
                          style={styles.baseBalanceText}
                        >
                          {formatBaseBalance(balance, a.current_balance?.currency_decimals)}
                        </Text>
                      </View>
                    ))
                  ) : (
                    <Text
                      lightColor="#667085"
                      darkColor="rgba(255,255,255,0.45)"
                      style={styles.noBalanceText}
                    >
                      No balance
                    </Text>
                  )}
                  {a.current_balance?.total_value_in_base !== undefined && (
                    <View style={styles.totalItem}>
                      <Text style={styles.totalLabel}>Total</Text>
                      <Text style={styles.totalText}>{formatAccountTotal(a.current_balance)}</Text>
                    </View>
                  )}
                </View>
              </View>
              <Text lightColor="#667085" darkColor="rgba(255,255,255,0.5)" style={styles.cardMeta}>
                id: {a.id}
              </Text>
              {a.current_balance && (
                <Text
                  lightColor="#667085"
                  darkColor="rgba(255,255,255,0.5)"
                  style={styles.cardMeta}
                >
                  updated: {formatTimestamp(a.current_balance.timestamp)}
                </Text>
              )}
            </View>
          ))}
        </View>
      ))}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
  content: { padding: 16, gap: 16 },
  group: { gap: 8 },
  groupTitle: { fontSize: 16, fontWeight: 'bold', marginBottom: 4 },
  card: { borderWidth: 1, borderColor: 'rgba(128,128,128,0.3)', borderRadius: 8, padding: 12, gap: 4 },
  cardHeader: { flexDirection: 'row', justifyContent: 'space-between', gap: 12 },
  cardTitle: { flex: 1, fontWeight: '600' },
  balanceBlock: { alignItems: 'flex-end', flexShrink: 0, maxWidth: '45%' },
  balanceItem: { alignItems: 'flex-end', marginBottom: 4 },
  balanceText: { fontWeight: '600', textAlign: 'right' },
  baseBalanceText: { fontSize: 12, textAlign: 'right' },
  totalItem: {
    alignItems: 'flex-end',
    borderTopWidth: 1,
    borderTopColor: 'rgba(128,128,128,0.3)',
    marginTop: 2,
    paddingTop: 6,
  },
  totalLabel: { color: '#667085', fontSize: 11, textAlign: 'right' },
  totalText: { fontWeight: '700', textAlign: 'right' },
  noBalanceText: { fontSize: 13, textAlign: 'right' },
  cardMeta: { fontSize: 13 },
  muted: { color: 'rgba(255,255,255,0.4)', textAlign: 'center', marginTop: 40 },
});
