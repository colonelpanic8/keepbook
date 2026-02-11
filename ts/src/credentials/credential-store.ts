/**
 * CredentialStore abstraction (port of Rust credentials/mod.rs).
 *
 * Provides async access to secrets by key name.
 * Implementations may be backed by `pass`, environment variables, keyrings, etc.
 */
export interface CredentialStore {
  /** Retrieve a secret by key. Returns `null` if not found. */
  get(key: string): Promise<string | null>;

  /** Store a secret under the given key. */
  set(key: string, value: string): Promise<void>;

  /** Whether this store supports writing. Read-only stores return `false`. */
  supportsWrite(): boolean;
}
