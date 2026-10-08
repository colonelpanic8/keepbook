import { TextInput } from "keepbook-design";

export const Labeled = () => (
  <div style={{ display: "grid", gap: 12, gridTemplateColumns: "repeat(2, minmax(0, 1fr))" }}>
    <TextInput label="Confidence" kind="number" value="0.70" />
    <TextInput label="Start" kind="date" value="2026-07-10" />
  </div>
);

export const Search = () => <TextInput kind="search" className="transaction-search-input" placeholder="Filter titles" />;

export const Prompt = () => (
  <TextInput multiline placeholder="Ask for a tag, ignore, or rename rule for the selected transactions." />
);

export const Small = () => <TextInput small label="Effective date" kind="date" value="2026-06-15" />;
