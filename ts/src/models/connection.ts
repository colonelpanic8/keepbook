import { Id } from './id.js';
import { IdGenerator, UuidIdGenerator } from './id-generator.js';
import { Clock, SystemClock } from '../clock.js';

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type ConnectionStatus = 'active' | 'error' | 'disconnected' | 'pending_reauth';

export type SyncStatus = 'success' | 'failed' | 'partial';

export interface LastSync {
  readonly at: Date;
  readonly status: SyncStatus;
  readonly error?: string;
}

/** Credential backend configuration (opaque for now). */
export type CredentialConfig = unknown;

export interface ConnectionConfig {
  readonly name: string;
  readonly synchronizer: string;
  readonly credentials?: CredentialConfig;
  readonly balance_staleness?: number;
}

export interface ConnectionStateType {
  readonly id: Id;
  readonly status: ConnectionStatus;
  readonly created_at: Date;
  readonly last_sync?: LastSync;
  readonly account_ids: Id[];
  readonly synchronizer_data: unknown;
}

export interface ConnectionType {
  readonly config: ConnectionConfig;
  readonly state: ConnectionStateType;
}

// ---------------------------------------------------------------------------
// JSON types
// ---------------------------------------------------------------------------

export interface LastSyncJSON {
  at: string;
  status: SyncStatus;
  error?: string;
}

export interface ConnectionStateJSON {
  id: string;
  status: ConnectionStatus;
  created_at: string;
  last_sync?: LastSyncJSON;
  account_ids: string[];
  synchronizer_data?: unknown;
}

export interface ConnectionConfigJSON {
  name: string;
  synchronizer: string;
  credentials?: CredentialConfig;
  balance_staleness?: number;
}

export interface ConnectionJSON {
  config: ConnectionConfigJSON;
  state: ConnectionStateJSON;
}

// ---------------------------------------------------------------------------
// ConnectionState namespace
// ---------------------------------------------------------------------------

export const ConnectionState = {
  /**
   * Create connection state with explicit id and timestamp.
   * Defaults: active status, no last_sync, empty account_ids, null synchronizer_data.
   */
  newWith(id: Id, createdAt: Date): ConnectionStateType {
    return {
      id,
      status: 'active',
      created_at: createdAt,
      account_ids: [],
      synchronizer_data: null,
    };
  },

  /**
   * Create connection state using injectable id generator and clock.
   */
  newWithGenerator(ids: IdGenerator, clock: Clock): ConnectionStateType {
    return ConnectionState.newWith(ids.newId(), clock.now());
  },

  /**
   * Serialize to JSON.
   * Omits synchronizer_data when null. Omits last_sync when undefined.
   */
  toJSON(state: ConnectionStateType): ConnectionStateJSON {
    const json: ConnectionStateJSON = {
      id: state.id.toJSON(),
      status: state.status,
      created_at: state.created_at.toISOString(),
      account_ids: state.account_ids.map((id) => id.toJSON()),
    };

    if (state.last_sync !== undefined) {
      const syncJson: LastSyncJSON = {
        at: state.last_sync.at.toISOString(),
        status: state.last_sync.status,
      };
      if (state.last_sync.error !== undefined) {
        syncJson.error = state.last_sync.error;
      }
      json.last_sync = syncJson;
    }

    if (state.synchronizer_data !== null) {
      json.synchronizer_data = state.synchronizer_data;
    }

    return json;
  },

  /**
   * Deserialize from JSON.
   */
  fromJSON(json: ConnectionStateJSON): ConnectionStateType {
    const state: ConnectionStateType = {
      id: Id.fromString(json.id),
      status: json.status,
      created_at: new Date(json.created_at),
      account_ids: json.account_ids.map((id) => Id.fromString(id)),
      synchronizer_data: json.synchronizer_data ?? null,
    };

    if (json.last_sync !== undefined) {
      const lastSync: LastSync = {
        at: new Date(json.last_sync.at),
        status: json.last_sync.status,
        ...(json.last_sync.error !== undefined ? { error: json.last_sync.error } : {}),
      };
      return { ...state, last_sync: lastSync };
    }

    return state;
  },
} as const;

// ---------------------------------------------------------------------------
// Connection namespace
// ---------------------------------------------------------------------------

export const Connection = {
  /**
   * Create a new connection from config, generating state with injectable deps.
   */
  new(config: ConnectionConfig, ids?: IdGenerator, clock?: Clock): ConnectionType {
    const state = ConnectionState.newWithGenerator(
      ids ?? new UuidIdGenerator(),
      clock ?? new SystemClock(),
    );
    return { config, state };
  },

  /** Get the connection id (delegates to state). */
  id(conn: ConnectionType): Id {
    return conn.state.id;
  },

  /** Get the connection name (delegates to config). */
  name(conn: ConnectionType): string {
    return conn.config.name;
  },

  /** Get the synchronizer name (delegates to config). */
  synchronizer(conn: ConnectionType): string {
    return conn.config.synchronizer;
  },

  /** Get the connection status (delegates to state). */
  status(conn: ConnectionType): ConnectionStatus {
    return conn.state.status;
  },

  /**
   * Serialize to JSON.
   */
  toJSON(conn: ConnectionType): ConnectionJSON {
    const configJson: ConnectionConfigJSON = {
      name: conn.config.name,
      synchronizer: conn.config.synchronizer,
    };
    if (conn.config.credentials !== undefined) {
      configJson.credentials = conn.config.credentials;
    }
    if (conn.config.balance_staleness !== undefined) {
      configJson.balance_staleness = conn.config.balance_staleness;
    }
    return {
      config: configJson,
      state: ConnectionState.toJSON(conn.state),
    };
  },

  /**
   * Deserialize from JSON.
   */
  fromJSON(json: ConnectionJSON): ConnectionType {
    const config: ConnectionConfig = {
      name: json.config.name,
      synchronizer: json.config.synchronizer,
      ...(json.config.credentials !== undefined ? { credentials: json.config.credentials } : {}),
      ...(json.config.balance_staleness !== undefined
        ? { balance_staleness: json.config.balance_staleness }
        : {}),
    };
    return {
      config,
      state: ConnectionState.fromJSON(json.state),
    };
  },
} as const;
