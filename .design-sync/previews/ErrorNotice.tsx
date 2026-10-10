import { ErrorNotice } from "keepbook-design";

export const GitSyncFailed = () => (
  <ErrorNotice message="Git sync failed: remote rejected the push. Pull first, then retry." />
);
