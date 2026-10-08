import { Badge, Checkbox, ControlButton, DataTable } from "keepbook-design";

export const Accounts = () => (
  <DataTable className="account-table" columns={["Account", "Balance (USD)", "Status", "Tags", "Include"]}>
    <div className="table-row account-row-with-toggle">
      <button className="account-row-main account-click-row" title="View graph">
        <strong>Demo Checking</strong>
      </button>
      <span>$8,250.44</span>
      <Badge tone="positive">Active</Badge>
      <small>cash</small>
      <div className="account-override-cell">
        <Checkbox label="Include" className="account-include-toggle" checked />
      </div>
    </div>
    <div className="table-row ignored-account-row account-row-with-toggle">
      <button className="account-row-main account-click-row" title="View graph">
        <strong>Old Savings</strong>
      </button>
      <span>$0.00</span>
      <Badge>Ignored</Badge>
      <small />
      <div className="account-override-cell">
        <Checkbox label="Include" className="account-include-toggle" />
        <ControlButton small title="Reset account override">
          Reset
        </ControlButton>
      </div>
    </div>
  </DataTable>
);

export const Connections = () => (
  <DataTable className="connection-table" columns={["Name", "Source", "Accounts", "Last balance refresh", "Actions"]}>
    <div className="table-row">
      <strong>Demo Bank</strong>
      <Badge tone="positive">active</Badge>
      <span>3</span>
      <small>2026-10-08 09:14</small>
      <div className="connection-actions">
        <ControlButton>Balances</ControlButton>
        <ControlButton>Prices</ControlButton>
      </div>
    </div>
    <div className="table-row">
      <strong>Brokerage</strong>
      <Badge tone="positive">active</Badge>
      <span>1</span>
      <small>Never</small>
      <div className="connection-actions">
        <ControlButton busy>Refreshing</ControlButton>
        <ControlButton disabled>Prices</ControlButton>
      </div>
    </div>
  </DataTable>
);
