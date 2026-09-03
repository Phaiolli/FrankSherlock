import { useEffect, useRef, useState } from "react";
import ModalOverlay from "./ModalOverlay";
import type { VolumeInfo } from "../../types";
import "./shared-modal.css";
import "./PickLocationModal.css";

type Props = {
  /** Mounted disks; empty while still loading. */
  volumes: VolumeInfo[];
  loading: boolean;
  /** Error from a hand-typed path, shown inline. */
  error: string | null;
  onCancel: () => void;
  /** Open the system dialog, optionally starting inside `startPath`. */
  onBrowse: (startPath?: string) => void;
  /** Use a hand-typed path directly. */
  onUsePath: (path: string) => void;
};

const KIND_GLYPH: Record<VolumeInfo["kind"], string> = {
  home: "⌂",
  drive: "▣",
  root: "∕",
};

export default function PickLocationModal({ volumes, loading, error, onCancel, onBrowse, onUsePath }: Props) {
  const [path, setPath] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (error) inputRef.current?.focus();
  }, [error]);

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (path.trim()) onUsePath(path);
    } else if (e.key === "Escape") {
      e.preventDefault();
      onCancel();
    }
  }

  return (
    <ModalOverlay onBackdropClick={onCancel}>
      <div className="modal-base pick-location-modal" onClick={(e) => e.stopPropagation()}>
        <h3>Add folder</h3>
        <p>
          Pick a disk to browse from — the system dialog often hides secondary
          drives — or type a path directly.
        </p>

        {loading && <p className="pick-location-empty">Looking for disks...</p>}

        <div className="pick-location-list">
          {volumes.map((v) => (
            <button
              key={v.path}
              type="button"
              className="pick-location-item"
              onClick={() => onBrowse(v.path)}
              title={`Browse ${v.path}`}
            >
              <span className="pick-location-glyph" aria-hidden="true">{KIND_GLYPH[v.kind]}</span>
              <span className="pick-location-labels">
                <span className="pick-location-name">{v.name}</span>
                <span className="pick-location-path">{v.path}</span>
              </span>
            </button>
          ))}
        </div>

        <label className="pick-location-manual" htmlFor="pick-location-path">Or type a folder path</label>
        <div className="pick-location-row">
          <input
            id="pick-location-path"
            ref={inputRef}
            type="text"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="/mnt/arquivos/fotos"
            aria-label="Folder path"
            spellCheck={false}
          />
          <button type="button" onClick={() => onUsePath(path)} disabled={!path.trim()}>Use path</button>
        </div>
        {error && <p className="pick-location-error">{error}</p>}

        <div className="modal-actions">
          <button type="button" onClick={onCancel}>Cancel</button>
          <button type="button" onClick={() => onBrowse()}>Browse...</button>
        </div>
      </div>
    </ModalOverlay>
  );
}
