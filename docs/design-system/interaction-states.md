# Interaction states

Keepbook stays usable while background work runs. An operation should explain
what is happening without replacing already-loaded content or making the whole
app appear frozen.

## In-flight operations

- Start user-triggered work asynchronously. Never run network, disk, sync, Git,
  market-data, or AI work on the UI thread.
- Show a nearby `OperationStatus` as soon as work starts. Use a concrete present
  participle such as “Refreshing prices…” or “Saving tags…”, plus an
  indeterminate spinner when exact progress is unavailable.
- Put `aria-busy="true"` on the active status and control. Announce status text
  with a polite live region; do not repeatedly announce animation frames.
- Keep navigation, already-loaded content, scrolling, filtering, and unrelated
  actions available. Disable only the initiating control and actions that would
  conflict with the same mutation.
- Preserve the current content while revalidating it. Initial loads may use a
  local skeleton or activity panel; background refreshes must not swap the page
  for a blank screen or full-screen spinner.
- Prevent duplicate submission while an operation is active. The initiating
  button shows a compact spinner and an active verb when space permits.

## Completion and recovery

- Replace progress text with a concise success or failure result. Do not leave a
  spinner running after the future resolves.
- A failed load offers an in-context retry. A failed mutation restores its
  controls and keeps the user's inputs whenever retrying is safe.
- Long operations that support safe cancellation (currently Git clone/sync)
  expose Cancel. Cancellation must be cooperative and must restore controls
  whether it succeeds, fails, or is canceled.
- If an operation has no safe cancellation mechanism, users must still be able
  to navigate elsewhere. Do not imply that closing a view cancels server work.

## Shared components

- `BackendActivity` is for initial, local data-region loading only.
- `GraphLoadingPanel` is the chart-specific initial/loading treatment.
- `OperationStatus` is the standard persistent feedback for user-triggered work.
- `ControlButton`'s `busy` state adds the compact spinner, `aria-busy`, and
  duplicate-submission protection.

Status treatments use semantic roles from `styles.css`: accent colors for
in-flight activity, neutral surfaces for settled messages, and the shared
spinner/progress tokens. Add new state colors only through the token document.
