import { Id } from '../models/id.js';
import { type Storage } from './storage.js';
import { type ConnectionType } from '../models/connection.js';
import { type AccountType } from '../models/account.js';

/**
 * Find a connection by ID or name (case-insensitive).
 *
 * If `idOrName` is path-safe, tries lookup by ID first. Falls back to
 * case-insensitive name search. Throws if multiple connections match by name.
 *
 * Port of Rust `find_connection` (storage/lookup.rs).
 */
export async function findConnection(
  storage: Storage,
  idOrName: string,
): Promise<ConnectionType | null> {
  // If path-safe, try lookup by ID first
  if (Id.isPathSafe(idOrName)) {
    const id = Id.fromString(idOrName);
    const conn = await storage.getConnection(id);
    if (conn !== null) {
      return conn;
    }
  }

  // Fall back to name search (case-insensitive)
  const connections = await storage.listConnections();
  const needle = idOrName.toLowerCase();
  const matches = connections.filter((conn) => conn.config.name.toLowerCase() === needle);

  if (matches.length === 0) {
    return null;
  }
  if (matches.length > 1) {
    const ids = matches.map((c) => c.state.id.asStr());
    throw new Error(
      `Multiple connections named '${idOrName}'. Use an ID instead: ${JSON.stringify(ids)}`,
    );
  }
  return matches[0];
}

/**
 * Find an account by ID or name (case-insensitive).
 *
 * If `idOrName` is path-safe, tries lookup by ID first. Falls back to
 * case-insensitive name search. Throws if multiple accounts match by name.
 *
 * Port of Rust `find_account` (storage/lookup.rs).
 */
export async function findAccount(storage: Storage, idOrName: string): Promise<AccountType | null> {
  // If path-safe, try lookup by ID first
  if (Id.isPathSafe(idOrName)) {
    const id = Id.fromString(idOrName);
    const account = await storage.getAccount(id);
    if (account !== null) {
      return account;
    }
  }

  // Fall back to name search (case-insensitive)
  const accounts = await storage.listAccounts();
  const needle = idOrName.toLowerCase();
  const matches = accounts.filter((acct) => acct.name.toLowerCase() === needle);

  if (matches.length === 0) {
    return null;
  }
  if (matches.length > 1) {
    const ids = matches.map((a) => a.id.asStr());
    throw new Error(
      `Multiple accounts named '${idOrName}'. Use an ID instead: ${JSON.stringify(ids)}`,
    );
  }
  return matches[0];
}
