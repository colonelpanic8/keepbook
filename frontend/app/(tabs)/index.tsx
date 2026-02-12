import { Button, ScrollView, StyleSheet } from 'react-native';
import { useCallback, useEffect, useMemo, useState } from 'react';
import AsyncStorage from '@react-native-async-storage/async-storage';
import { useFocusEffect } from 'expo-router';

import EditScreenInfo from '@/components/EditScreenInfo';
import { Text, View } from '@/components/Themed';
import KeepbookNative from '@/modules/keepbook-native';

type ConnectionSummary = {
  id: string;
  name: string;
  synchronizer: string;
  status: string;
  created_at: string;
  last_sync_at?: string | null;
  last_sync_status?: string | null;
};

type AccountSummary = {
  id: string;
  name: string;
  connection_id: string;
  created_at: string;
  active: boolean;
};

export default function TabOneScreen() {
  const version = KeepbookNative.version();
  const hello = KeepbookNative.hello();

  const demoDir = KeepbookNative.demoDataDir();
  const gitDir = KeepbookNative.gitDataDir();

  const [dataDir, setDataDir] = useState<string>(() => demoDir);
  const [initError, setInitError] = useState<string>('');
  const [connectionsJson, setConnectionsJson] = useState<string>('[]');
  const [accountsJson, setAccountsJson] = useState<string>('[]');

  const refreshDir = useCallback(async (dir: string) => {
    const [connections, accounts] = await Promise.all([
      KeepbookNative.listConnections(dir),
      KeepbookNative.listAccounts(dir),
    ]);
    setConnectionsJson(connections);
    setAccountsJson(accounts);
  }, []);

  const parsedConnections = useMemo(() => {
    try {
      const v: unknown = JSON.parse(connectionsJson);
      if (Array.isArray(v)) return { items: v as ConnectionSummary[], error: '' };
      if (v && typeof v === 'object' && 'error' in v) return { items: [], error: String((v as any).error) };
      return { items: [], error: 'unexpected response' };
    } catch (e) {
      return { items: [], error: `invalid JSON: ${String(e)}` };
    }
  }, [connectionsJson]);

  const parsedAccounts = useMemo(() => {
    try {
      const v: unknown = JSON.parse(accountsJson);
      if (Array.isArray(v)) return { items: v as AccountSummary[], error: '' };
      if (v && typeof v === 'object' && 'error' in v) return { items: [], error: String((v as any).error) };
      return { items: [], error: 'unexpected response' };
    } catch (e) {
      return { items: [], error: `invalid JSON: ${String(e)}` };
    }
  }, [accountsJson]);

  useEffect(() => {
    void (async () => {
      const saved = await AsyncStorage.getItem('keepbook.data_dir');
      const next = saved || demoDir;
      setDataDir(next);
      await refreshDir(next);
    })();
  }, [demoDir, refreshDir]);

  // If another tab switches the selected data dir (e.g. after a git sync),
  // pick that up when this screen is focused.
  useFocusEffect(
    useCallback(() => {
      let cancelled = false;

      void (async () => {
        const saved = await AsyncStorage.getItem('keepbook.data_dir');
        const next = saved || demoDir;

        if (cancelled) return;

        setDataDir(next);
        await refreshDir(next);
      })();

      return () => {
        cancelled = true;
      };
    }, [demoDir, refreshDir])
  );

  const initDemo = async () => {
    setInitError('');
    const err = await KeepbookNative.initDemo(demoDir);
    setInitError(err);
    await refreshDir(demoDir);
  };

  const refresh = async () => {
    await refreshDir(dataDir);
  };

  const useDemo = async () => {
    setDataDir(demoDir);
    await AsyncStorage.setItem('keepbook.data_dir', demoDir);
    await refreshDir(demoDir);
  };

  const useGit = async () => {
    setDataDir(gitDir);
    await AsyncStorage.setItem('keepbook.data_dir', gitDir);
    await refreshDir(gitDir);
  };

  return (
    <ScrollView contentContainerStyle={styles.container}>
      <Text style={styles.title}>keepbook</Text>
      <View style={styles.separator} lightColor="#eee" darkColor="rgba(255,255,255,0.1)" />

      <Text>keepbook runtime: {version}</Text>
      <Text>{hello}</Text>
      <Text>Data dir: {dataDir}</Text>

      <View style={styles.buttonRow}>
        <Button title="Init demo data" onPress={() => void initDemo()} />
        <Button title="Refresh" onPress={() => void refresh()} />
      </View>

      <View style={styles.buttonRow}>
        <Button title="Use demo data" onPress={() => void useDemo()} />
        <Button title="Use git data" onPress={() => void useGit()} />
      </View>

      {initError ? (
        <Text style={styles.errorText}>Init error: {initError}</Text>
      ) : null}

      <Text style={styles.sectionTitle}>Connections</Text>
      {parsedConnections.error ? (
        <Text style={styles.errorText}>Error: {parsedConnections.error}</Text>
      ) : parsedConnections.items.length ? (
        parsedConnections.items.map((c) => (
          <View key={c.id} style={styles.card}>
            <Text style={styles.cardTitle}>{c.name}</Text>
            <Text style={styles.cardMeta}>
              {c.status} · {c.synchronizer}
            </Text>
            {c.last_sync_at ? (
              <Text style={styles.cardMeta}>
                last sync: {c.last_sync_at} ({c.last_sync_status || 'unknown'})
              </Text>
            ) : null}
            <Text style={styles.cardMeta}>id: {c.id}</Text>
          </View>
        ))
      ) : (
        <Text style={styles.muted}>No connections.</Text>
      )}

      <Text style={styles.sectionTitle}>Accounts</Text>
      {parsedAccounts.error ? (
        <Text style={styles.errorText}>Error: {parsedAccounts.error}</Text>
      ) : parsedAccounts.items.length ? (
        parsedAccounts.items.map((a) => (
          <View key={a.id} style={styles.card}>
            <Text style={styles.cardTitle}>
              {a.name} {a.active ? '' : '(inactive)'}
            </Text>
            <Text style={styles.cardMeta}>connection: {a.connection_id}</Text>
            <Text style={styles.cardMeta}>id: {a.id}</Text>
          </View>
        ))
      ) : (
        <Text style={styles.muted}>No accounts.</Text>
      )}

      <Text style={styles.sectionTitle}>Debug</Text>
      <Text style={styles.muted}>Raw JSON output (for now):</Text>
      <Text selectable style={styles.mono}>
        {connectionsJson}
      </Text>
      <Text selectable style={styles.mono}>
        {accountsJson}
      </Text>

      <EditScreenInfo path="app/(tabs)/index.tsx" />
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    alignItems: 'stretch',
    justifyContent: 'flex-start',
    gap: 12,
    paddingHorizontal: 16,
    paddingVertical: 24,
  },
  title: {
    fontSize: 20,
    fontWeight: 'bold',
  },
  separator: {
    marginVertical: 30,
    height: 1,
    width: '80%',
  },
  sectionTitle: {
    marginTop: 12,
    fontSize: 16,
    fontWeight: 'bold',
    alignSelf: 'flex-start',
  },
  mono: {
    fontFamily: 'SpaceMono',
    fontSize: 12,
    alignSelf: 'stretch',
  },
  errorText: {
    color: '#c92a2a',
    alignSelf: 'stretch',
  },
  muted: {
    color: '#666',
    alignSelf: 'stretch',
  },
  buttonRow: {
    flexDirection: 'row',
    gap: 12,
  },
  card: {
    alignSelf: 'stretch',
    borderWidth: 1,
    borderColor: '#ddd',
    borderRadius: 12,
    padding: 12,
    gap: 4,
  },
  cardTitle: {
    fontWeight: '700',
  },
  cardMeta: {
    color: '#555',
  },
});
