import { describe, it, expect, beforeEach } from 'vitest';
import { MemoryStorage } from './memory.js';
import { Id } from '../models/id.js';
import { Account, type AccountType } from '../models/account.js';
import {
  Connection,
  ConnectionState,
  type ConnectionConfig,
  type ConnectionType,
} from '../models/connection.js';
import { findConnection, findAccount } from './lookup.js';

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeConnection(name: string, id?: Id, createdAt?: Date): ConnectionType {
  const connId = id ?? Id.new();
  const ts = createdAt ?? new Date('2024-01-01T00:00:00Z');
  const config: ConnectionConfig = { name, synchronizer: 'test-sync' };
  const state = ConnectionState.newWith(connId, ts);
  return { config, state };
}

function makeAccount(name: string, connectionId: Id, id?: Id, createdAt?: Date): AccountType {
  const acctId = id ?? Id.new();
  const ts = createdAt ?? new Date('2024-01-01T00:00:00Z');
  return Account.newWith(acctId, ts, name, connectionId);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('findConnection', () => {
  let storage: MemoryStorage;

  beforeEach(() => {
    storage = new MemoryStorage();
  });

  it('returns null for unknown id or name', async () => {
    const result = await findConnection(storage, 'nonexistent');
    expect(result).toBeNull();
  });

  it('finds by ID', async () => {
    const id = Id.fromString('conn-abc-123');
    const conn = makeConnection('My Bank', id);
    await storage.saveConnection(conn);

    const result = await findConnection(storage, 'conn-abc-123');
    expect(result).not.toBeNull();
    expect(Connection.name(result!)).toBe('My Bank');
    expect(Connection.id(result!).equals(id)).toBe(true);
  });

  it('finds by name (case-insensitive)', async () => {
    const conn = makeConnection('My Bank');
    await storage.saveConnection(conn);

    // Exact case
    const exact = await findConnection(storage, 'My Bank');
    expect(exact).not.toBeNull();
    expect(Connection.name(exact!)).toBe('My Bank');

    // Different case
    const lower = await findConnection(storage, 'my bank');
    expect(lower).not.toBeNull();
    expect(Connection.name(lower!)).toBe('My Bank');

    const upper = await findConnection(storage, 'MY BANK');
    expect(upper).not.toBeNull();
    expect(Connection.name(upper!)).toBe('My Bank');
  });

  it('throws on duplicate names', async () => {
    const conn1 = makeConnection('Duplicate');
    const conn2 = makeConnection('Duplicate');
    await storage.saveConnection(conn1);
    await storage.saveConnection(conn2);

    await expect(findConnection(storage, 'Duplicate')).rejects.toThrow(/[Mm]ultiple connections/);
  });

  it('prefers ID match over name match', async () => {
    // Create a connection whose ID string is the same as another connection's name
    const idStr = 'special-id';
    const id = Id.fromString(idStr);
    const connById = makeConnection('Connection By Id', id);
    await storage.saveConnection(connById);

    // Create another connection whose name matches the ID string
    const connByName = makeConnection(idStr);
    await storage.saveConnection(connByName);

    // Lookup should prefer the ID match
    const result = await findConnection(storage, idStr);
    expect(result).not.toBeNull();
    expect(Connection.id(result!).equals(id)).toBe(true);
    expect(Connection.name(result!)).toBe('Connection By Id');
  });
});

describe('findAccount', () => {
  let storage: MemoryStorage;
  let connectionId: Id;

  beforeEach(async () => {
    storage = new MemoryStorage();
    connectionId = Id.new();
    const conn = makeConnection('Test Connection', connectionId);
    await storage.saveConnection(conn);
  });

  it('returns null for unknown id or name', async () => {
    const result = await findAccount(storage, 'nonexistent');
    expect(result).toBeNull();
  });

  it('finds by ID', async () => {
    const id = Id.fromString('acct-abc-123');
    const acct = makeAccount('Checking', connectionId, id);
    await storage.saveAccount(acct);

    const result = await findAccount(storage, 'acct-abc-123');
    expect(result).not.toBeNull();
    expect(result!.name).toBe('Checking');
    expect(result!.id.equals(id)).toBe(true);
  });

  it('finds by name (case-insensitive)', async () => {
    const acct = makeAccount('Checking', connectionId);
    await storage.saveAccount(acct);

    // Exact case
    const exact = await findAccount(storage, 'Checking');
    expect(exact).not.toBeNull();
    expect(exact!.name).toBe('Checking');

    // Different case
    const lower = await findAccount(storage, 'checking');
    expect(lower).not.toBeNull();
    expect(lower!.name).toBe('Checking');

    const upper = await findAccount(storage, 'CHECKING');
    expect(upper).not.toBeNull();
    expect(upper!.name).toBe('Checking');
  });

  it('throws on duplicate names', async () => {
    const acct1 = makeAccount('Duplicate', connectionId);
    const acct2 = makeAccount('Duplicate', connectionId);
    await storage.saveAccount(acct1);
    await storage.saveAccount(acct2);

    await expect(findAccount(storage, 'Duplicate')).rejects.toThrow(/[Mm]ultiple accounts/);
  });

  it('prefers ID match over name match', async () => {
    // Create an account whose ID string is the same as another account's name
    const idStr = 'special-id';
    const id = Id.fromString(idStr);
    const acctById = makeAccount('Account By Id', connectionId, id);
    await storage.saveAccount(acctById);

    // Create another account whose name matches the ID string
    const acctByName = makeAccount(idStr, connectionId);
    await storage.saveAccount(acctByName);

    // Lookup should prefer the ID match
    const result = await findAccount(storage, idStr);
    expect(result).not.toBeNull();
    expect(result!.id.equals(id)).toBe(true);
    expect(result!.name).toBe('Account By Id');
  });
});
