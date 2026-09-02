import type { RootInfo } from "../../types";
import ModalOverlay from "./ModalOverlay";
import "./shared-modal.css";
import "./ConfirmDeleteModal.css";

type Props = {
  root: RootInfo;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
};

export default function OrganizeModal({ root, busy, onCancel, onConfirm }: Props) {
  return (
    <ModalOverlay onBackdropClick={busy ? undefined : onCancel}>
      <div className="modal-base confirm-modal" onClick={(e) => e.stopPropagation()}>
        <h3>Organize by people?</h3>
        <p>
          Every file in <strong>{root.rootName}</strong> with a recognised person will be moved
          to <code>Pessoas/&lt;Name&gt;/</code> inside the folder. A photo with several people is
          copied into each person&apos;s folder.
        </p>
        <p className="confirm-path">{root.rootPath}</p>
        <p className="confirm-note">
          Files without recognised people stay where they are. Folders left empty are removed.
          The index is updated in place, so no rescan is needed. This cannot be undone automatically.
        </p>
        <div className="modal-actions">
          <button type="button" onClick={onCancel} disabled={busy}>Cancel</button>
          <button type="button" onClick={onConfirm} disabled={busy}>
            {busy ? "Organizing..." : "Organize"}
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
