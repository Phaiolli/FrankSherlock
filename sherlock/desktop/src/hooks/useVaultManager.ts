import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { attachVault, createVault, getVaultSupport, listVolumes, lockVault, probeVault, resolveFolderPath, unlockVault } from "../api";
import type { RootInfo, SetupStatus, VaultProbe, VaultSupport, VolumeInfo } from "../types";
import { errorMessage } from "../utils";

type VaultManagerCallbacks = {
  onNotice: (msg: string) => void;
  onError: (msg: string) => void;
  /** Start a full scan for a folder that was just added (plain or vault). */
  scanPath: (rootPath: string) => Promise<void>;
  refreshRoots: () => Promise<void>;
  /** Called after a lock/unlock so listings and stats can be re-queried. */
  onVisibilityChanged: () => Promise<void> | void;
};

/**
 * Owns the "add folder" flow (plain or secret) and vault lock/unlock state.
 * Modals read `pendingFolder` / `unlockTarget`; App only wires them up.
 */
export function useVaultManager(cb: VaultManagerCallbacks) {
  const [support, setSupport] = useState<VaultSupport | null>(null);
  const [pendingFolder, setPendingFolder] = useState<string | null>(null);
  /** Result of probing `pendingFolder` for an existing encrypted store. */
  const [probe, setProbe] = useState<VaultProbe | null>(null);
  const [unlockTarget, setUnlockTarget] = useState<RootInfo | null>(null);
  const [busy, setBusy] = useState(false);
  /** True while the "where to browse" chooser is open. */
  const [picking, setPicking] = useState(false);
  const [volumes, setVolumes] = useState<VolumeInfo[]>([]);
  const [volumesLoading, setVolumesLoading] = useState(false);
  /** Inline error for a hand-typed path in the chooser. */
  const [pathError, setPathError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    getVaultSupport()
      .then((s) => { if (!cancelled) setSupport(s); })
      .catch((err) => { if (!cancelled) setSupport({ supported: false, reason: errorMessage(err) }); });
    return () => { cancelled = true; };
  }, []);

  /** Hand a chosen path to the Add Folder modal, probing it for a vault. */
  const acceptFolder = useCallback(async (path: string) => {
    setPicking(false);
    setPathError(null);
    setProbe(null);
    setPendingFolder(path);
    // Detect a previously created vault (".name.vault" next to, or as, the pick).
    const result = await probeVault(path).catch(() => null);
    setProbe(result);
  }, []);

  /**
   * Open the location chooser. System dialogs rarely list secondary disks, so
   * the user first picks a volume (or types a path) and only then browses.
   */
  const onPickFolder = useCallback(async (setup: SetupStatus | null, readOnly: boolean) => {
    if (readOnly) return;
    if (setup && !setup.isReady) {
      cb.onError("Setup is incomplete. Finish Ollama setup before adding folders.");
      return;
    }
    setPathError(null);
    setPicking(true);
    setVolumesLoading(true);
    try {
      setVolumes(await listVolumes());
    } catch {
      setVolumes([]); // Browsing and typing a path still work without the list.
    } finally {
      setVolumesLoading(false);
    }
  }, [cb]);

  /** Open the native picker, starting inside `startPath` when given. */
  const browseFrom = useCallback(async (startPath?: string) => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Select folder to add",
        ...(startPath ? { defaultPath: startPath } : {}),
      });
      if (!selected) return; // Cancelled: leave the chooser open.
      await acceptFolder(selected as string);
    } catch (err) {
      cb.onError(errorMessage(err));
    }
  }, [cb, acceptFolder]);

  /** Accept a hand-typed path once the backend confirms it is a folder. */
  const useTypedPath = useCallback(async (input: string) => {
    try {
      await acceptFolder(await resolveFolderPath(input));
    } catch (err) {
      setPathError(errorMessage(err));
    }
  }, [acceptFolder]);

  const cancelPickLocation = useCallback(() => { setPicking(false); setPathError(null); }, []);

  const cancelAddFolder = useCallback(() => { setPendingFolder(null); setProbe(null); }, []);

  /** Add the pending folder as a normal (unencrypted) root and scan it. */
  const onAddPlainFolder = useCallback(async () => {
    const path = pendingFolder;
    setPendingFolder(null);
    if (path) await cb.scanPath(path);
  }, [pendingFolder, cb]);

  /**
   * Convert the pending folder into an encrypted vault, then scan it.
   * Returns an error message to show inline, or null on success.
   */
  const onCreateVault = useCallback(async (password: string): Promise<string | null> => {
    const path = pendingFolder;
    if (!path) return "No folder selected";
    setBusy(true);
    try {
      const result = await createVault(path, password);
      setPendingFolder(null);
      cb.onNotice(
        result.migratedFiles > 0
          ? `Secret folder created: ${result.migratedFiles} files encrypted`
          : "Secret folder created",
      );
      await cb.scanPath(result.rootPath);
      return null;
    } catch (err) {
      return errorMessage(err);
    } finally {
      setBusy(false);
    }
  }, [pendingFolder, cb]);

  /**
   * Reopen an existing encrypted store for the pending path, then scan it.
   * Returns an error message to show inline, or null on success.
   */
  const onAttachVault = useCallback(async (password: string): Promise<string | null> => {
    const path = pendingFolder;
    if (!path) return "No folder selected";
    setBusy(true);
    try {
      const result = await attachVault(path, password);
      setPendingFolder(null);
      setProbe(null);
      cb.onNotice(
        result.migratedFiles > 0
          ? `Secret folder reopened: ${result.migratedFiles} files restored from its index`
          : "Secret folder reopened",
      );
      await cb.scanPath(result.rootPath);
      return null;
    } catch (err) {
      return errorMessage(err);
    } finally {
      setBusy(false);
    }
  }, [pendingFolder, cb]);

  const cancelUnlock = useCallback(() => setUnlockTarget(null), []);

  /** Unlock `unlockTarget`. Returns an inline error message or null. */
  const onUnlock = useCallback(async (password: string): Promise<string | null> => {
    const root = unlockTarget;
    if (!root) return "No folder selected";
    setBusy(true);
    try {
      await unlockVault(root.id, password);
      setUnlockTarget(null);
      cb.onNotice(`Unlocked "${root.rootName}"`);
      await cb.refreshRoots();
      await cb.onVisibilityChanged();
      return null;
    } catch (err) {
      return errorMessage(err);
    } finally {
      setBusy(false);
    }
  }, [unlockTarget, cb]);

  const onLock = useCallback(async (root: RootInfo) => {
    setBusy(true);
    try {
      await lockVault(root.id);
      cb.onNotice(`Locked "${root.rootName}"`);
      await cb.refreshRoots();
      await cb.onVisibilityChanged();
    } catch (err) {
      cb.onError(errorMessage(err));
    } finally {
      setBusy(false);
    }
  }, [cb]);

  return {
    support,
    busy,
    pendingFolder,
    probe,
    unlockTarget,
    setUnlockTarget,
    onPickFolder,
    picking,
    volumes,
    volumesLoading,
    pathError,
    browseFrom,
    useTypedPath,
    cancelPickLocation,
    cancelAddFolder,
    onAddPlainFolder,
    onCreateVault,
    onAttachVault,
    cancelUnlock,
    onUnlock,
    onLock,
  };
}
