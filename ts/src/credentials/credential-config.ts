/**
 * CredentialConfig types and parsing (port of Rust credentials/config.rs).
 *
 * A discriminated union tagged on `backend`. Currently only the "pass" variant
 * is supported.
 */
import * as toml from 'toml';

/** Configuration for the `pass` credential backend. */
export interface PassConfig {
  backend: 'pass';
  /** Base path in the password store (e.g. "finance/coinbase-api"). */
  path: string;
  /** Maps credential key names to sub-paths within the pass entry. */
  fields: Record<string, string>;
}

/**
 * Discriminated union of all supported credential backends.
 * Extend with additional variants as needed.
 */
export type CredentialConfig = PassConfig;

/**
 * Parse a TOML string into a `CredentialConfig`.
 *
 * Expected TOML format:
 * ```toml
 * backend = "pass"
 * path = "finance/coinbase-api"
 *
 * [fields]
 * key_name = "key-name"
 * private_key = "private-key"
 * ```
 *
 * @throws Error if the TOML is invalid or required fields are missing.
 */
export function parseCredentialConfig(input: string): CredentialConfig {
  const raw = toml.parse(input) as unknown;
  return parseCredentialConfigValue(raw);
}

/**
 * Parse a value (typically already TOML-parsed) into a `CredentialConfig`.
 *
 * This is useful when credential config is embedded inside another TOML file
 * (e.g. `connection.toml` has a `[credentials]` table).
 */
export function parseCredentialConfigValue(input: unknown): CredentialConfig {
  if (input === null || typeof input !== 'object' || Array.isArray(input)) {
    throw new Error('Credential config must be a table/object');
  }

  const raw = input as Record<string, unknown>;

  const backend = raw['backend'];
  if (typeof backend !== 'string') {
    throw new Error('Missing or invalid "backend" field in credential config');
  }

  if (backend !== 'pass') {
    throw new Error(`Unsupported credential backend: "${backend}"`);
  }

  const path = raw['path'];
  if (typeof path !== 'string') {
    throw new Error('Missing or invalid "path" field in credential config');
  }

  const rawFields = raw['fields'];
  let fields: Record<string, string>;

  if (rawFields === undefined || rawFields === null) {
    fields = {};
  } else if (typeof rawFields === 'object' && !Array.isArray(rawFields)) {
    fields = {};
    for (const [k, v] of Object.entries(rawFields as Record<string, unknown>)) {
      if (typeof v !== 'string') {
        throw new Error(`Field "${k}" must be a string, got ${typeof v}`);
      }
      fields[k] = v;
    }
  } else {
    throw new Error('"fields" must be a table/object');
  }

  return { backend: 'pass', path, fields };
}
