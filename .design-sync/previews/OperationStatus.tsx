import { OperationStatus } from "keepbook-design";

export const Busy = () => <OperationStatus busy message="Refreshing prices…" />;

export const Done = () => <OperationStatus message="Refreshed 14 prices." />;

export const Dismissible = () => <OperationStatus message="Data resynced." onDismiss={() => {}} />;

export const Failed = () => <OperationStatus message="Git sync failed: remote rejected the push. Pull first, then retry." />;
