# Abstraction Sweep Plan

## Goals
- Make the CLI thin by pushing behavior into reusable services.
- Consolidate sync entrypoints into one orchestrated path.
- Abstract interactive auth, storage, and market-data so tests can substitute mocks.
- Reduce type-specific branching (Schwab/Chase/Coinbase) in `app.rs`.

## Non-goals (for this sweep)
- Redesign data model or storage format.
- Change external CLI output fields or error messages unless required by refactor.
- Replace existing synchronizer logic (only route through interfaces).

## Current Issues (Critical Assessment)
- `sync_connection` in `src/app.rs` still knows too much about specific synchronizers.
- Auth prompting lives in `app.rs`, tied to stdin/stdout, which blocks clean reuse.
- Sync orchestration (save + price storage + price refresh + auto-commit) is split across layers.
- Construction requirements (e.g., Chase download dir) are not encoded in an interface.
- Tests rely on CLI entrypoints or ad-hoc helpers instead of a single service API.

## Target Architecture
- `SyncService` (or an upgraded `SyncOrchestrator`) is the single entrypoint.
- `SyncContext` carries shared dependencies:
  - `storage: Arc<dyn Storage>`
  - `market_data: MarketDataService`
  - `data_dir: PathBuf`
  - `auth_prompter: Arc<dyn AuthPrompter>`
- `AuthPrompter` is a trait; CLI provides stdin implementation; tests provide mock.
- Synchronizer creation is centralized through a factory using `SyncContext`.
- CLI functions call service methods only; no type-specific branching in `app.rs`.

## Plan of Attack (One Sweep)
1. Introduce service types and traits.
2. Move auth prompting into `AuthPrompter` and integrate into the service.
3. Centralize synchronizer construction using `SyncContext`.
4. Refactor `sync_connection`, `sync_all`, and `sync_connection_if_stale` to use the service.
5. Update tests to use the service API and mock prompter.
6. Remove dead helpers and ensure existing CLI output stays stable.

## Risks
- API churn across synchronizer constructors or trait signatures.
- Test brittleness due to CLI output changes.
- Accidental changes to interactive behavior (auth prompts).

## Test Strategy
- Keep existing tests green.
- Add service-level tests for:
  - auth prompting behavior
  - sync + price storage + price refresh pipeline
  - CLI remains thin (no direct calls to synchronizers)
