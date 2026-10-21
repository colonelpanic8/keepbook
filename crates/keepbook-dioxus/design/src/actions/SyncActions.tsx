import { SplitButton } from "./SplitButton";
import type { ButtonFeedback } from "./ControlButton";

export type SyncAction =
  | { kind: "balances"; ifStale: boolean; fullTransactions: boolean }
  | { kind: "prices"; force: boolean };

export interface SyncActionsProps {
  disabled?: boolean;
  balancesFeedback?: ButtonFeedback;
  pricesFeedback?: ButtonFeedback;
  onAction?: (action: SyncAction) => void;
}

export function SyncActions({ disabled = false, balancesFeedback, pricesFeedback, onAction }: SyncActionsProps) {
  return (
    <div className="connection-actions">
      <SplitButton
        icon="refresh"
        title="Refresh balances and recent transactions for stale connections"
        menuLabel="More balance refresh options"
        disabled={disabled}
        feedback={balancesFeedback}
        actions={[
          { value: "all", label: "Refresh all balances", detail: "Refresh even if balances are still fresh" },
          { value: "full", label: "Refresh full history", detail: "Refresh balances and all available transactions" },
          { value: "stale-full", label: "Refresh stale full history", detail: "Fetch all transactions only for stale connections" },
        ]}
        onClick={() => onAction?.({ kind: "balances", ifStale: true, fullTransactions: false })}
        onSelect={(value) => {
          switch (value) {
            case "all": onAction?.({ kind: "balances", ifStale: false, fullTransactions: false }); break;
            case "full": onAction?.({ kind: "balances", ifStale: false, fullTransactions: true }); break;
            case "stale-full": onAction?.({ kind: "balances", ifStale: true, fullTransactions: true }); break;
          }
        }}
      >Balances</SplitButton>
      <SplitButton
        icon="refresh"
        title="Refetch every price, ignoring staleness"
        menuLabel="More price refresh options"
        disabled={disabled}
        feedback={pricesFeedback}
        actions={[{ value: "stale-prices", label: "Refresh stale prices", detail: "Skip prices that are still fresh" }]}
        onClick={() => onAction?.({ kind: "prices", force: true })}
        onSelect={(value) => {
          switch (value) {
            case "stale-prices": onAction?.({ kind: "prices", force: false }); break;
          }
        }}
      >Prices</SplitButton>
    </div>
  );
}
