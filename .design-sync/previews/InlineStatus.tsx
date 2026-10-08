import { InlineStatus } from "keepbook-design";

export const LoadFailed = () => (
  <InlineStatus title="Couldn't load spending" message="keepbook-server didn't respond. Check that it's running, then refresh." />
);
