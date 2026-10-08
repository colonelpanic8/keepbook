import { DataTable, TagPill } from "keepbook-design";

export const Transactions = () => (
  <DataTable
    columns={[
      { label: "Date", width: "120px" },
      { label: "Description", width: "minmax(180px, 1.2fr)" },
      { label: "Tags", width: "minmax(120px, 0.8fr)" },
      { label: "Account", width: "minmax(140px, 0.8fr)" },
      { label: "Amount", width: "110px", align: "end" },
    ]}
    rows={[
      ["2026-07-06", <strong>WHOLE FOODS MARKET</strong>, <TagPill tag="Groceries" />, "Demo Credit Card", <strong>-$88.42</strong>],
      ["2026-07-03", <strong>TARTINE BAKERY</strong>, <TagPill tag="Dining" />, "Demo Credit Card", <strong>-$38.20</strong>],
      ["2026-07-01", <strong>Park Ave Apartments Rent</strong>, <TagPill tag="Housing" />, "Demo Checking", <strong>-$2,400.00</strong>],
      ["2026-06-18", <strong>PG&E Utilities</strong>, <TagPill tag="Utilities" />, "Demo Checking", <strong>-$124.09</strong>],
    ]}
    onRowClick={() => {}}
  />
);
