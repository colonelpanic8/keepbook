import React, { useCallback, useState } from 'react';
import { ScrollView, StyleSheet } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useFocusEffect } from 'expo-router';
import { Text, View } from '@/components/Themed';
import KeepbookNative from '@/modules/keepbook-native';

type AccountSummary = { id: string; name: string; connection_id: string; created_at: string; active: boolean };
type ConnectionSummary = { id: string; name: string; synchronizer: string; status: string };

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
              <Text style={styles.cardTitle}>{a.name}{a.active ? '' : ' (inactive)'}</Text>
              <Text style={styles.cardMeta}>id: {a.id}</Text>
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
  card: { borderWidth: 1, borderColor: 'rgba(255,255,255,0.15)', borderRadius: 10, padding: 12, gap: 4 },
  cardTitle: { fontWeight: '600' },
  cardMeta: { color: 'rgba(255,255,255,0.5)', fontSize: 13 },
  muted: { color: 'rgba(255,255,255,0.4)', textAlign: 'center', marginTop: 40 },
});
