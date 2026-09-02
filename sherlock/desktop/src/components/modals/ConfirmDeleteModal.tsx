import type { RootInfo } from "../../types";
import ModalOverlay from "./ModalOverlay";
import "./shared-modal.css";
import "./ConfirmDeleteModal.css";

type Props = {
  root: RootInfo;
  isScanning?: boolean;
  onCancel: () => void;
  onConfirm: (root: RootInfo) => void;
};

export default function ConfirmDeleteModal({ root, isScanning, onCancel, onConfirm }: Props) {
  return (
    <ModalOverlay onBackdropClick={onCancel}>
      <div className="modal-base confirm-modal" onClick={(e) => e.stopPropagation()}>
        <h3>Remove folder?</h3>
        <p>
          This will remove <strong>{root.rootName}</strong> and
          all {root.fileCount} indexed files from the database and cache.
        </p>
        <p className="confirm-path">{root.rootPath}</p>
        {isScanning && (
          <p className="confirm-scan-warning">A scan is running for this folder and will be cancelled.</p>
        )}
        {root.isVault ? (
          <p className="confirm-note">
            The secret folder will be locked first. Its encrypted store (<code>.{root.rootName}.vault</code>)
            stays on disk and can be reopened later with its password.
          </p>
        ) : (
          <p className="confirm-note">Original files on disk will not be touched.</p>
        )}
        <div className="modal-actions">
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" className="danger-btn" onClick={() => onConfirm(root)}>
            Remove
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
