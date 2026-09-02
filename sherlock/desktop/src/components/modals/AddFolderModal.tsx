import { useEffect, useRef, useState } from "react";
import ModalOverlay from "./ModalOverlay";
import type { VaultProbe, VaultSupport } from "../../types";
import { basename } from "../../utils";
import "./shared-modal.css";
import "./VaultModal.css";

type Props = {
  folderPath: string;
  vaultSupport: VaultSupport | null;
  /** Null while probing; `attachable` switches the modal to "reopen" mode. */
  probe?: VaultProbe | null;
  busy: boolean;
  onCancel: () => void;
  onAddPlain: () => void;
  /** Resolves to an error message to display, or null when done. */
  onCreateVault: (password: string) => Promise<string | null>;
  /** Reopen the existing encrypted store; same contract as onCreateVault. */
  onAttachVault?: (password: string) => Promise<string | null>;
};

const MIN_PASSWORD_LEN = 4;

export function validateVaultPassword(password: string, confirm: string): string | null {
  if (password.length < MIN_PASSWORD_LEN) return `Password must be at least ${MIN_PASSWORD_LEN} characters`;
  if (password !== confirm) return "Passwords do not match";
  return null;
}

export default function AddFolderModal({ folderPath, vaultSupport, probe, busy, onCancel, onAddPlain, onCreateVault, onAttachVault }: Props) {
  const [secret, setSecret] = useState(false);
  const [password, setPassword] = useState("");
  const [confirm, setConfirm] = useState("");
  const [acknowledged, setAcknowledged] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const passwordRef = useRef<HTMLInputElement>(null);

  const supported = vaultSupport?.supported ?? false;
  const folderName = basename(folderPath);
  const reopen = !!(probe?.attachable && onAttachVault);

  useEffect(() => {
    if (secret || reopen) passwordRef.current?.focus();
  }, [secret, reopen]);

  async function handleSubmit() {
    if (busy) return;
    if (reopen) {
      if (!password) {
        setError("Enter the vault password");
        return;
      }
      const result = await onAttachVault!(password);
      if (result) {
        setError(result);
        setPassword("");
      }
      return;
    }
    if (!secret) {
      onAddPlain();
      return;
    }
    const err = validateVaultPassword(password, confirm);
    if (err) {
      setError(err);
      return;
    }
    if (!acknowledged) {
      setError("Please confirm you understand the password cannot be recovered");
      return;
    }
    const result = await onCreateVault(password);
    if (result) setError(result);
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
        <h3>{reopen ? "Reopen secret folder" : "Add folder"}</h3>
        <p className="vault-path" title={folderPath}>{folderPath}</p>

        {reopen && (
          <>
            <p className="vault-warning">
              An encrypted store already exists for <strong>{basename(probe?.mountPoint ?? folderPath)}</strong>.
              Enter its password to reopen it. Its previous index (descriptions, text) is restored
              if it was sealed inside the vault.
            </p>
            <input
              ref={passwordRef}
              type="password"
              value={password}
              placeholder="Password"
              autoComplete="current-password"
              disabled={busy}
              onChange={(e) => { setPassword(e.target.value); setError(null); }}
              onKeyDown={handleKeyDown}
              aria-label="Vault password"
            />
          </>
        )}

        {!reopen && <label className="vault-checkbox">
          <input
            type="checkbox"
            checked={secret}
            disabled={!supported || busy}
            onChange={(e) => { setSecret(e.target.checked); setError(null); }}
            aria-label="Secret folder"
          />
          <span>Secret folder (encrypted, password protected)</span>
        </label>}
        {!reopen && !supported && (
          <p className="vault-hint">
            {vaultSupport === null ? "Checking vault support..." : vaultSupport.reason ?? "Encrypted folders are not available on this system."}
          </p>
        )}

        {!reopen && secret && (
          <>
            <input
              ref={passwordRef}
              type="password"
              value={password}
              placeholder="Password"
              autoComplete="new-password"
              disabled={busy}
              onChange={(e) => { setPassword(e.target.value); setError(null); }}
              onKeyDown={handleKeyDown}
              aria-label="Vault password"
            />
            <input
              type="password"
              value={confirm}
              placeholder="Confirm password"
              autoComplete="new-password"
              disabled={busy}
              onChange={(e) => { setConfirm(e.target.value); setError(null); }}
              onKeyDown={handleKeyDown}
              aria-label="Confirm vault password"
            />
            <p className="vault-warning">
              Files in <strong>{folderName}</strong> will be moved into an encrypted store
              (<code>.{folderName}.vault</code>, next to the folder) and the unencrypted originals
              deleted after the copy is verified. The folder is only readable while it is unlocked
              in Frank Sherlock; it is locked automatically when the app closes.
            </p>
            <label className="vault-checkbox">
              <input
                type="checkbox"
                checked={acknowledged}
                disabled={busy}
                onChange={(e) => { setAcknowledged(e.target.checked); setError(null); }}
                aria-label="I understand the password cannot be recovered"
              />
              <span>I understand that if I forget the password, the files cannot be recovered.</span>
            </label>
          </>
        )}

        {error && <p className="vault-error" role="alert">{error}</p>}

        <div className="modal-actions">
          <button type="button" onClick={onCancel} disabled={busy}>Cancel</button>
          <button type="button" onClick={() => void handleSubmit()} disabled={busy}>
            {busy ? (reopen ? "Opening..." : "Encrypting...") : reopen ? "Reopen secret folder" : secret ? "Create secret folder" : "Add folder"}
          </button>
        </div>
      </div>
    </ModalOverlay>
  );
}
