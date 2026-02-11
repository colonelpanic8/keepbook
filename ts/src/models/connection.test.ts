import { describe, it, expect } from 'vitest';
import { Id } from './id.js';
import { FixedIdGenerator } from './id-generator.js';
import { FixedClock } from '../clock.js';
import {
  ConnectionState,
  Connection,
  type ConnectionConfig,
  type ConnectionStatus,
  type SyncStatus,
  type LastSync,
  type CredentialConfig,
  type ConnectionStateType,
} from './connection.js';

describe('ConnectionStatus', () => {
  it('has the expected string literal values', () => {
    const statuses: ConnectionStatus[] = ['active', 'error', 'disconnected', 'pending_reauth'];
    expect(statuses).toHaveLength(4);
  });
});

describe('SyncStatus', () => {
  it('has the expected string literal values', () => {
    const statuses: SyncStatus[] = ['success', 'failed', 'partial'];
    expect(statuses).toHaveLength(3);
  });
});

describe('LastSync', () => {
  it('can represent a successful sync', () => {
    const sync: LastSync = {
      at: new Date('2024-06-15T12:00:00.000Z'),
      status: 'success',
    };
    expect(sync.at.toISOString()).toBe('2024-06-15T12:00:00.000Z');
    expect(sync.status).toBe('success');
    expect(sync.error).toBeUndefined();
  });

  it('can represent a failed sync with error', () => {
    const sync: LastSync = {
      at: new Date('2024-06-15T12:00:00.000Z'),
      status: 'failed',
      error: 'Connection timeout',
    };
    expect(sync.status).toBe('failed');
    expect(sync.error).toBe('Connection timeout');
  });
});

describe('ConnectionConfig', () => {
  it('can be created with required fields only', () => {
    const config: ConnectionConfig = {
      name: 'My Bank',
      synchronizer: 'plaid',
    };
    expect(config.name).toBe('My Bank');
    expect(config.synchronizer).toBe('plaid');
    expect(config.credentials).toBeUndefined();
    expect(config.balance_staleness).toBeUndefined();
  });

  it('can be created with all fields', () => {
    const config: ConnectionConfig = {
      name: 'My Bank',
      synchronizer: 'plaid',
      credentials: { type: 'env', key: 'PLAID_TOKEN' } as CredentialConfig,
      balance_staleness: 86400000,
    };
    expect(config.credentials).toEqual({ type: 'env', key: 'PLAID_TOKEN' });
    expect(config.balance_staleness).toBe(86400000);
  });
});

describe('ConnectionState', () => {
  const fixedId = Id.fromString('test-conn-id');
  const fixedDate = new Date('2024-01-01T00:00:00.000Z');

  describe('newWith', () => {
    it('creates state with explicit id and timestamp', () => {
      const state = ConnectionState.newWith(fixedId, fixedDate);

      expect(state.id.equals(fixedId)).toBe(true);
      expect(state.created_at.getTime()).toBe(fixedDate.getTime());
      expect(state.status).toBe('active');
      expect(state.last_sync).toBeUndefined();
      expect(state.account_ids).toEqual([]);
      expect(state.synchronizer_data).toBeNull();
    });
  });

  describe('newWithGenerator', () => {
    it('uses injected id generator and clock', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);

      const state = ConnectionState.newWithGenerator(ids, clock);

      expect(state.id.equals(fixedId)).toBe(true);
      expect(state.created_at.getTime()).toBe(fixedDate.getTime());
      expect(state.status).toBe('active');
      expect(state.last_sync).toBeUndefined();
      expect(state.account_ids).toEqual([]);
      expect(state.synchronizer_data).toBeNull();
    });
  });

  describe('JSON serialization', () => {
    it('serializes with snake_case fields', () => {
      const state = ConnectionState.newWith(fixedId, fixedDate);
      const json = ConnectionState.toJSON(state);

      expect(json.id).toBe('test-conn-id');
      expect(json.created_at).toBe('2024-01-01T00:00:00.000Z');
      expect(json.status).toBe('active');
      expect(json.account_ids).toEqual([]);
    });

    it('omits synchronizer_data when null', () => {
      const state = ConnectionState.newWith(fixedId, fixedDate);
      const json = ConnectionState.toJSON(state);

      expect('synchronizer_data' in json).toBe(false);
    });

    it('includes last_sync when present', () => {
      const state = ConnectionState.newWith(fixedId, fixedDate);
      const withSync: ConnectionStateType = {
        ...state,
        last_sync: {
          at: new Date('2024-06-15T12:00:00.000Z'),
          status: 'success',
        },
      };
      const json = ConnectionState.toJSON(withSync);

      expect(json.last_sync).toEqual({
        at: '2024-06-15T12:00:00.000Z',
        status: 'success',
      });
    });

    it('includes last_sync error when present', () => {
      const state = ConnectionState.newWith(fixedId, fixedDate);
      const withSync: ConnectionStateType = {
        ...state,
        last_sync: {
          at: new Date('2024-06-15T12:00:00.000Z'),
          status: 'failed',
          error: 'timeout',
        },
      };
      const json = ConnectionState.toJSON(withSync);

      expect(json.last_sync).toEqual({
        at: '2024-06-15T12:00:00.000Z',
        status: 'failed',
        error: 'timeout',
      });
    });

    it('round-trips through JSON', () => {
      const state = ConnectionState.newWith(fixedId, fixedDate);
      const accountId = Id.fromString('acct-1');
      const withAccounts: ConnectionStateType = {
        ...state,
        account_ids: [accountId],
        last_sync: {
          at: new Date('2024-06-15T12:00:00.000Z'),
          status: 'partial',
          error: 'some accounts failed',
        },
      };
      const json = ConnectionState.toJSON(withAccounts);
      const parsed = ConnectionState.fromJSON(json);

      expect(parsed.id.equals(fixedId)).toBe(true);
      expect(parsed.created_at.getTime()).toBe(fixedDate.getTime());
      expect(parsed.status).toBe('active');
      expect(parsed.account_ids).toHaveLength(1);
      expect(parsed.account_ids[0].equals(accountId)).toBe(true);
      expect(parsed.last_sync?.status).toBe('partial');
      expect(parsed.last_sync?.error).toBe('some accounts failed');
      expect(parsed.synchronizer_data).toBeNull();
    });
  });
});

describe('Connection', () => {
  const fixedId = Id.fromString('conn-id');
  const fixedDate = new Date('2024-01-01T00:00:00.000Z');
  const config: ConnectionConfig = {
    name: 'Chase Bank',
    synchronizer: 'plaid',
  };

  describe('new', () => {
    it('creates a connection with config and generates state', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);

      const conn = Connection.new(config, ids, clock);

      expect(conn.config).toBe(config);
      expect(conn.state.id.equals(fixedId)).toBe(true);
      expect(conn.state.status).toBe('active');
    });
  });

  describe('accessor functions', () => {
    const ids = new FixedIdGenerator([fixedId]);
    const clock = new FixedClock(fixedDate);
    const conn = Connection.new(config, ids, clock);

    it('id returns the state id', () => {
      expect(Connection.id(conn).equals(fixedId)).toBe(true);
    });

    it('name returns the config name', () => {
      expect(Connection.name(conn)).toBe('Chase Bank');
    });

    it('synchronizer returns the config synchronizer', () => {
      expect(Connection.synchronizer(conn)).toBe('plaid');
    });

    it('status returns the state status', () => {
      expect(Connection.status(conn)).toBe('active');
    });
  });

  describe('JSON serialization', () => {
    it('serializes correctly', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);
      const conn = Connection.new(config, ids, clock);
      const json = Connection.toJSON(conn);

      expect(json.config.name).toBe('Chase Bank');
      expect(json.config.synchronizer).toBe('plaid');
      expect(json.state.id).toBe('conn-id');
      expect(json.state.status).toBe('active');
    });

    it('round-trips through JSON', () => {
      const ids = new FixedIdGenerator([fixedId]);
      const clock = new FixedClock(fixedDate);
      const conn = Connection.new(config, ids, clock);
      const json = Connection.toJSON(conn);
      const parsed = Connection.fromJSON(json);

      expect(Connection.id(parsed).equals(fixedId)).toBe(true);
      expect(Connection.name(parsed)).toBe('Chase Bank');
      expect(Connection.synchronizer(parsed)).toBe('plaid');
      expect(Connection.status(parsed)).toBe('active');
    });
  });
});
