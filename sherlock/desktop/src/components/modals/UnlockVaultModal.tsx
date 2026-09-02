import { useEffect, useRef, useState } from "react";
import ModalOverlay from "./ModalOverlay";
import type { RootInfo } from "../../types";
import "./shared-modal.css";
import "./VaultModal.css";

type Props = {
  root: RootInfo;
  busy: boolean;
  onCancel: () => void;
  /** Resolves to an error message to display, or null when unlocked. */
  onUnlock: (password: string) => Promise<string | null>;
};

export default function UnlockVaultModal({ root, busy, onCancel, onUnlock }: Props) {
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  async function handleSubmit() {
    if (busy) return;
    if (!password) {
      setError("Enter the password");
      return;
    }
    const result = await onUnlock(password);
    if (result) {
      setError(result);
      setPassword("");
      inputRef.current?.focus();
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      void handleSubmit();
    }
  }

  return (
    <ModalOverlay onBackdropClick={busy ? undefined : onCancel}>
      <div className="modal-base vault-modal" onClick={(e) => e.stopPropagation()}>
        <h3>Unlock secret folder</h3>
        <p className="vault-path" title={root.rootPath}>{root.rootName}</p>
        <input
          ref={inputRef}
          type="password"
          value={password}
          placeholder="Password"
          autoComplete="current-password"
          disabled={busy}
          onChange={(e) => { setPassword(e.target.value); setError(null); }}
          onKeyDown={handleKeyDown}
          aria-label="Vault password"
        />
        {error && <p className="vault-error" role="alert">{error}</p>}
        <div className="modal-actions">
          <button type="button" onClick={onCancel} disabled={busy}>Cancel</button>
          <button type="button" onClick={() => void handleSubmit()} disabled={busy}>
            {busy ? "Unlocking..." : "Unlock"}
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
