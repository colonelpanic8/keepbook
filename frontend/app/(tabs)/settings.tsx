import { useEffect, useState } from 'react';
import { Button, Platform, ScrollView, StyleSheet, TextInput } from 'react-native';
import AsyncStorage from '@react-native-async-storage/async-storage';
import * as SecureStore from 'expo-secure-store';

import { Text, View } from '@/components/Themed';
import KeepbookNative from '@/modules/keepbook-native';

export default function SettingsScreen() {
  const [gitHost, setGitHost] = useState('github.com');
  const [gitRepo, setGitRepo] = useState('colonelpanic8/keepbook-data');
  const [gitBranch, setGitBranch] = useState('main');
  const [sshUser, setSshUser] = useState('git');
  const [sshPrivateKey, setSshPrivateKey] = useState('');
  const [githubToken, setGithubToken] = useState('');
  const [status, setStatus] = useState<string>('');
  const repoDir = KeepbookNative.gitRepoDir();
  const dataDir = KeepbookNative.gitDataDir();

  useEffect(() => {
    void (async () => {
      try {
        const [host, repo, branch, user] = await Promise.all([
          AsyncStorage.getItem('keepbook.git.host'),
          AsyncStorage.getItem('keepbook.git.repo'),
          AsyncStorage.getItem('keepbook.git.branch'),
          AsyncStorage.getItem('keepbook.git.ssh_user'),
        ]);

        if (host) setGitHost(host);
        if (repo) setGitRepo(repo);
        if (branch) setGitBranch(branch);
        if (user) setSshUser(user);

        // SecureStore isn't always available (e.g. web); treat it as best-effort.
        try {
          const [key, token] = await Promise.all([
            SecureStore.getItemAsync('keepbook.git.ssh_private_key'),
            SecureStore.getItemAsync('keepbook.git.github_token'),
          ]);
          if (key) {
            setSshPrivateKey(key);
          } else if (Platform.OS === 'web') {
            const fallback = await AsyncStorage.getItem('keepbook.git.ssh_private_key');
            if (fallback) setSshPrivateKey(fallback);
          }
          if (token) {
            setGithubToken(token);
          } else if (Platform.OS === 'web') {
            const fallback = await AsyncStorage.getItem('keepbook.git.github_token');
            if (fallback) setGithubToken(fallback);
          }
        } catch {
          if (Platform.OS === 'web') {
            const [keyFallback, tokenFallback] = await Promise.all([
              AsyncStorage.getItem('keepbook.git.ssh_private_key'),
              AsyncStorage.getItem('keepbook.git.github_token'),
            ]);
            if (keyFallback) setSshPrivateKey(keyFallback);
            if (tokenFallback) setGithubToken(tokenFallback);
          }
        }
      } catch (e) {
        setStatus(`Load failed: ${String(e)}`);
      }
    })();
  }, []);

  const save = async () => {
    setStatus('');
    try {
      await Promise.all([
        AsyncStorage.setItem('keepbook.git.host', gitHost.trim()),
        AsyncStorage.setItem('keepbook.git.repo', gitRepo.trim()),
        AsyncStorage.setItem('keepbook.git.branch', gitBranch.trim() || 'main'),
        AsyncStorage.setItem('keepbook.git.ssh_user', sshUser.trim()),
      ]);

      try {
        if (sshPrivateKey.trim()) {
          await SecureStore.setItemAsync('keepbook.git.ssh_private_key', sshPrivateKey);
        } else {
          await SecureStore.deleteItemAsync('keepbook.git.ssh_private_key');
        }
      } catch (e) {
        // If SecureStore is unavailable, fall back to AsyncStorage on web only.
        if (Platform.OS === 'web') {
          await AsyncStorage.setItem('keepbook.git.ssh_private_key', sshPrivateKey);
        } else {
          throw e;
        }
      }

      try {
        if (githubToken.trim()) {
          await SecureStore.setItemAsync('keepbook.git.github_token', githubToken.trim());
        } else {
          await SecureStore.deleteItemAsync('keepbook.git.github_token');
        }
      } catch (e) {
        if (Platform.OS === 'web') {
          await AsyncStorage.setItem('keepbook.git.github_token', githubToken.trim());
        } else {
          throw e;
        }
      }

      setStatus('Saved.');
    } catch (e) {
      setStatus(`Save failed: ${String(e)}`);
    }
  };

  const clearKey = async () => {
    setSshPrivateKey('');
    setStatus('');
    try {
      await SecureStore.deleteItemAsync('keepbook.git.ssh_private_key');
      if (Platform.OS === 'web') {
        await AsyncStorage.removeItem('keepbook.git.ssh_private_key');
      }
      setStatus('SSH key cleared.');
    } catch (e) {
      setStatus(`Clear failed: ${String(e)}`);
    }
  };

  const clearToken = async () => {
    setGithubToken('');
    setStatus('');
    try {
      await SecureStore.deleteItemAsync('keepbook.git.github_token');
      if (Platform.OS === 'web') {
        await AsyncStorage.removeItem('keepbook.git.github_token');
      }
      setStatus('GitHub token cleared.');
    } catch (e) {
      setStatus(`Clear failed: ${String(e)}`);
    }
  };

  const sync = async () => {
    setStatus('');
    try {
      const err = await KeepbookNative.gitSync(
        repoDir,
        gitHost.trim(),
        gitRepo.trim(),
        sshUser.trim(),
        sshPrivateKey,
        (gitBranch.trim() || 'main'),
        githubToken.trim()
      );
      if (err) {
        setStatus(`Sync failed: ${err}`);
        return;
      }
      await AsyncStorage.setItem('keepbook.data_dir', dataDir);
      setStatus(`Synced into ${repoDir}. Using data dir: ${dataDir}`);
    } catch (e) {
      setStatus(`Sync failed: ${String(e)}`);
    }
  };

  return (
    <ScrollView contentContainerStyle={styles.container}>
      <Text style={styles.title}>Git Sync (WIP)</Text>
      <View style={styles.separator} lightColor="#eee" darkColor="rgba(255,255,255,0.1)" />

      {repoDir ? <Text selectable>Repo dir: {repoDir}</Text> : null}
      {dataDir ? <Text selectable>Data dir: {dataDir}</Text> : null}

      <Text style={styles.label}>Git host</Text>
      <TextInput
        style={styles.input}
        autoCapitalize="none"
        autoCorrect={false}
        value={gitHost}
        onChangeText={setGitHost}
        placeholder="github.com"
      />

      <Text style={styles.label}>Repo (owner/name)</Text>
      <TextInput
        style={styles.input}
        autoCapitalize="none"
        autoCorrect={false}
        value={gitRepo}
        onChangeText={setGitRepo}
        placeholder="user/keepbook-data"
      />

      <Text style={styles.label}>Branch</Text>
      <TextInput
        style={styles.input}
        autoCapitalize="none"
        autoCorrect={false}
        value={gitBranch}
        onChangeText={setGitBranch}
        placeholder="main"
      />

      <Text style={styles.label}>SSH user</Text>
      <TextInput
        style={styles.input}
        autoCapitalize="none"
        autoCorrect={false}
        value={sshUser}
        onChangeText={setSshUser}
        placeholder="git"
      />

      <Text style={styles.label}>SSH private key (PEM)</Text>
      <TextInput
        style={[styles.input, styles.multiline]}
        autoCapitalize="none"
        autoCorrect={false}
        value={sshPrivateKey}
        onChangeText={setSshPrivateKey}
        placeholder={'-----BEGIN OPENSSH PRIVATE KEY-----\n...\n-----END OPENSSH PRIVATE KEY-----'}
        multiline
      />
      <Text style={styles.note}>
        TS sync is read-only and uses GitHub HTTP (tree via api.github.com + content via raw.githubusercontent.com). SSH
        key is currently ignored.
      </Text>

      <Text style={styles.label}>GitHub token (for private repos, web sync)</Text>
      <TextInput
        style={styles.input}
        autoCapitalize="none"
        autoCorrect={false}
        value={githubToken}
        onChangeText={setGithubToken}
        placeholder="ghp_... / github_pat_..."
      />

      <View style={styles.buttonRow}>
        <Button title="Save" onPress={() => void save()} />
        <Button title="Clear key" onPress={() => void clearKey()} />
        <Button title="Clear token" onPress={() => void clearToken()} />
        <Button title="Sync" onPress={() => void sync()} />
      </View>

      {status ? (
        <Text style={styles.status} selectable>
          {status}
        </Text>
      ) : null}
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
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
  label: {
    alignSelf: 'flex-start',
    fontWeight: '600',
  },
  input: {
    alignSelf: 'stretch',
    borderWidth: 1,
    borderColor: '#ccc',
    borderRadius: 8,
    padding: 10,
  },
  multiline: {
    minHeight: 140,
    textAlignVertical: 'top',
  },
  buttonRow: {
    flexDirection: 'row',
    gap: 12,
  },
  status: {
    alignSelf: 'stretch',
    color: '#555',
  },
  note: {
    color: '#666',
  },
});
