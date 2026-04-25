import AsyncStorage from '@react-native-async-storage/async-storage';

const DB_NAME = 'keepbook-file-content';
const STORE_NAME = 'files';

function hasIndexedDb(): boolean {
  return typeof globalThis.indexedDB !== 'undefined';
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = globalThis.indexedDB.open(DB_NAME, 1);

    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(STORE_NAME)) {
        db.createObjectStore(STORE_NAME);
      }
    };
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(request.error);
  });
}

async function getIndexedDbItem(key: string): Promise<string | null> {
  const db = await openDb();
  try {
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readonly');
      const request = tx.objectStore(STORE_NAME).get(key);
      request.onsuccess = () => {
        resolve(typeof request.result === 'string' ? request.result : null);
      };
      request.onerror = () => reject(request.error);
    });
  } finally {
    db.close();
  }
}

async function setIndexedDbItems(entries: Array<[string, string]>): Promise<void> {
  if (entries.length === 0) return;

  const db = await openDb();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readwrite');
      const store = tx.objectStore(STORE_NAME);

      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);

      for (const [key, value] of entries) {
        store.put(value, key);
      }
    });
  } finally {
    db.close();
  }
}

async function removeIndexedDbItemsWithPrefix(prefix: string): Promise<void> {
  const db = await openDb();
  try {
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readwrite');
      const store = tx.objectStore(STORE_NAME);
      const request = store.openCursor();

      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);

      request.onsuccess = () => {
        const cursor = request.result;
        if (!cursor) return;

        if (typeof cursor.key === 'string' && cursor.key.startsWith(prefix)) {
          cursor.delete();
        }
        cursor.continue();
      };
      request.onerror = () => reject(request.error);
    });
  } finally {
    db.close();
  }
}

export async function getFileContent(key: string): Promise<string | null> {
  if (hasIndexedDb()) {
    const value = await getIndexedDbItem(key);
    if (value !== null) return value;
  }

  return AsyncStorage.getItem(key);
}

export async function setFileContents(entries: Array<[string, string]>): Promise<void> {
  if (entries.length === 0) return;

  if (hasIndexedDb()) {
    await setIndexedDbItems(entries);
    return;
  }

  await AsyncStorage.multiSet(entries);
}

export async function removeFileContentsWithPrefix(prefix: string): Promise<void> {
  if (hasIndexedDb()) {
    await removeIndexedDbItemsWithPrefix(prefix);
  }

  const asyncStorageKeys = await AsyncStorage.getAllKeys();
  const staleKeys = asyncStorageKeys.filter((key) => key.startsWith(prefix));
  if (staleKeys.length > 0) {
    await AsyncStorage.multiRemove(staleKeys);
  }
}
