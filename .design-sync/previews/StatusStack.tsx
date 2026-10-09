import { OperationStatus, StatusStack } from "keepbook-design";

export const Stack = () => (
  // The stack is fixed to the viewport; a transformed box gives it a containing block inside the card.
  <div style={{ height: 200, transform: "translateZ(0)" }}>
    <StatusStack>
      <OperationStatus busy message="Refreshing prices…" />
      <OperationStatus message="Refreshed 14 prices." onDismiss={() => {}} />
    </StatusStack>
  </div>
);
