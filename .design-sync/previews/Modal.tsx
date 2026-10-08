import { ControlButton, IconButton, Modal, Progress } from "keepbook-design";

export const CloneRepository = () => (
  // The backdrop is fixed; a transformed box gives it a containing block inside the card.
  <div style={{ height: 320, transform: "translateZ(0)" }}>
    <Modal
      title="Clone repository"
      headerActions={<IconButton label="Close" glyph="×" />}
      actions={<ControlButton danger>Cancel</ControlButton>}
    >
      <Progress label="Cloning keepbook-data from github.com…" />
    </Modal>
  </div>
);
