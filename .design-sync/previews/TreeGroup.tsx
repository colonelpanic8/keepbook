import { Badge, Checkbox, DataTable, TreeGroup } from "keepbook-design";

const accounts = [
  { name: "Demo Checking", balance: "$8,250.44", tags: "cash", included: true },
  { name: "Demo Credit Card", balance: "-$1,436.20", tags: "card", included: true },
  { name: "Old Savings", balance: "$0.00", tags: "", included: false },
];

export const ConnectionAccounts = () => (
  <TreeGroup title="Demo Bank" subtitle="manual" aside={<Badge tone="positive">2/3 active</Badge>}>
    <DataTable className="account-table" columns={["Account", "Balance (USD)", "Status", "Tags", "Include"]}>
      {accounts.map((a) => (
        <div key={a.name} className={a.included ? "table-row account-row-with-toggle" : "table-row ignored-account-row account-row-with-toggle"}>
          <button className="account-row-main account-click-row" title="View graph">
            <strong>{a.name}</strong>
          </button>
          <span>{a.balance}</span>
          {a.included ? <Badge tone="positive">Active</Badge> : <Badge>Ignored</Badge>}
          <small>{a.tags}</small>
          <div className="account-override-cell">
            <Checkbox label="Include" className="account-include-toggle" checked={a.included} />
          </div>
        </div>
      ))}
    </DataTable>
  </TreeGroup>
);
