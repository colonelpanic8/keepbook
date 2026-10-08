import { Select } from "keepbook-design";

export const Labeled = () => <Select label="Sort" options={["Annual cost", "Name", "Next date"]} value="Annual cost" />;

export const Bare = () => <Select options={["Auto", "System", "Hidden"]} value="Auto" />;
