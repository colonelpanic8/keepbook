import { Badge, Checkbox, DataTable, TreeGroup } from "keepbook-design";

export const ConnectionAccounts = () => (
  <TreeGroup title="Demo Bank" subtitle="manual" aside={<Badge tone="positive">2/3 active</Badge>}>
    <DataTable
      columns={[
        { label: "Account", width: "minmax(160px, 1.2fr)" },
        { label: "Balance (USD)", width: "minmax(120px, 0.8fr)", align: "end" },
        { label: "Status", width: "100px" },
        { label: "Include", width: "110px" },
      ]}
      rows={[
        [<strong>Demo Checking</strong>, <strong>$8,250.44</strong>, <Badge tone="positive">Active</Badge>, <Checkbox label="Included" checked />],
        [<strong>Demo Credit Card</strong>, <strong>-$1,436.20</strong>, <Badge tone="positive">Active</Badge>, <Checkbox label="Included" checked />],
        [<strong>Old Savings</strong>, <strong>$0.00</strong>, <Badge>Ignored</Badge>, <Checkbox label="Included" />],
      ]}
      mutedRows={[2]}
    />
  </TreeGroup>
);
