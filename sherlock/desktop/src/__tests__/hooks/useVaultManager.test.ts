import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act, waitFor } from "@testing-library/react";
import { open } from "@tauri-apps/plugin-dialog";
import { useVaultManager } from "../../hooks/useVaultManager";
import { attachVault, createVault, getVaultSupport, lockVault, probeVault, unlockVault } from "../../api";
import { mockVaultRoot } from "../fixtures";

vi.mock("../../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../api")>();
  return {
    ...actual,
    getVaultSupport: vi.fn(),
    probeVault: vi.fn(),
    attachVault: vi.fn(),
    createVault: vi.fn(),
    unlockVault: vi.fn(),
    lockVault: vi.fn(),
  };
});

describe("useVaultManager", () => {
  const callbacks = {
    onNotice: vi.fn(),
    onError: vi.fn(),
    scanPath: vi.fn().mockResolvedValue(undefined),
    refreshRoots: vi.fn().mockResolvedValue(undefined),
    onVisibilityChanged: vi.fn().mockResolvedValue(undefined),
  };
  const readySetup = { isReady: true } as never;

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(getVaultSupport).mockResolvedValue({ supported: true, reason: null });
    vi.mocked(createVault).mockResolvedValue({ rootId: 7, rootPath: "/home/user/secret", migratedFiles: 3 });
    vi.mocked(probeVault).mockResolvedValue({ attachable: false, mountPoint: null, cipherDir: null });
    vi.mocked(attachVault).mockResolvedValue({ rootId: 8, rootPath: "/home/user/secret", migratedFiles: 12 });
    vi.mocked(unlockVault).mockResolvedValue(undefined);
    vi.mocked(lockVault).mockResolvedValue(undefined);
    vi.mocked(open).mockResolvedValue("/home/user/secret");
  });

  it("loads vault support on mount", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    await waitFor(() => expect(result.current.support).toEqual({ supported: true, reason: null }));
  });

  it("picking a folder opens the add-folder modal instead of scanning", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, false); });
    expect(result.current.pendingFolder).toBe("/home/user/secret");
    expect(probeVault).toHaveBeenCalledWith("/home/user/secret");
    expect(result.current.probe).toEqual({ attachable: false, mountPoint: null, cipherDir: null });
    expect(callbacks.scanPath).not.toHaveBeenCalled();
  });

  it("reopening an existing vault attaches it, scans it and notifies", async () => {
    vi.mocked(probeVault).mockResolvedValue({ attachable: true, mountPoint: "/home/user/secret", cipherDir: "/home/user/.secret.vault" });
    vi.mocked(open).mockResolvedValue("/home/user/.secret.vault");
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, false); });
    expect(result.current.probe?.attachable).toBe(true);
    let err: string | null = "unset";
    await act(async () => { err = await result.current.onAttachVault("hunter2"); });
    expect(err).toBeNull();
    expect(attachVault).toHaveBeenCalledWith("/home/user/.secret.vault", "hunter2");
    expect(callbacks.scanPath).toHaveBeenCalledWith("/home/user/secret");
    expect(callbacks.onNotice).toHaveBeenCalledWith(expect.stringContaining("12 files restored"));
    expect(result.current.pendingFolder).toBeNull();
    expect(result.current.probe).toBeNull();
  });

  it("attach errors are returned inline", async () => {
    vi.mocked(attachVault).mockRejectedValue("Incorrect password");
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, false); });
    let err: string | null = null;
    await act(async () => { err = await result.current.onAttachVault("nope"); });
    expect(err).toBe("Incorrect password");
    expect(result.current.pendingFolder).toBe("/home/user/secret");
  });

  it("refuses to pick when read-only or setup incomplete", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, true); });
    expect(open).not.toHaveBeenCalled();
    await act(async () => { await result.current.onPickFolder({ isReady: false } as never, false); });
    expect(open).not.toHaveBeenCalled();
    expect(callbacks.onError).toHaveBeenCalledWith(expect.stringContaining("Setup is incomplete"));
  });

  it("adding a plain folder scans it and closes the modal", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, false); });
    await act(async () => { await result.current.onAddPlainFolder(); });
    expect(callbacks.scanPath).toHaveBeenCalledWith("/home/user/secret");
    expect(result.current.pendingFolder).toBeNull();
  });

  it("creating a vault encrypts, scans the new root and notifies", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, false); });
    let err: string | null = "unset";
    await act(async () => { err = await result.current.onCreateVault("hunter2"); });
    expect(err).toBeNull();
    expect(createVault).toHaveBeenCalledWith("/home/user/secret", "hunter2");
    expect(callbacks.scanPath).toHaveBeenCalledWith("/home/user/secret");
    expect(callbacks.onNotice).toHaveBeenCalledWith(expect.stringContaining("3 files encrypted"));
    expect(result.current.pendingFolder).toBeNull();
  });

  it("returns backend errors from createVault and keeps the modal open", async () => {
    vi.mocked(createVault).mockRejectedValue(new Error("gocryptfs is not installed"));
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onPickFolder(readySetup, false); });
    let err: string | null = null;
    await act(async () => { err = await result.current.onCreateVault("hunter2"); });
    expect(err).toBe("gocryptfs is not installed");
    expect(result.current.pendingFolder).toBe("/home/user/secret");
    expect(callbacks.scanPath).not.toHaveBeenCalled();
  });

  it("unlock refreshes roots and listings on success", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    act(() => result.current.setUnlockTarget(mockVaultRoot));
    let err: string | null = "unset";
    await act(async () => { err = await result.current.onUnlock("hunter2"); });
    expect(err).toBeNull();
    expect(unlockVault).toHaveBeenCalledWith(7, "hunter2");
    expect(callbacks.refreshRoots).toHaveBeenCalled();
    expect(callbacks.onVisibilityChanged).toHaveBeenCalled();
    expect(result.current.unlockTarget).toBeNull();
  });

  it("unlock surfaces a wrong password inline", async () => {
    vi.mocked(unlockVault).mockRejectedValue("Incorrect password");
    const { result } = renderHook(() => useVaultManager(callbacks));
    act(() => result.current.setUnlockTarget(mockVaultRoot));
    let err: string | null = null;
    await act(async () => { err = await result.current.onUnlock("nope"); });
    expect(err).toBe("Incorrect password");
    expect(result.current.unlockTarget).toEqual(mockVaultRoot);
    expect(callbacks.refreshRoots).not.toHaveBeenCalled();
  });

  it("lock calls the backend and refreshes; errors go to toast", async () => {
    const { result } = renderHook(() => useVaultManager(callbacks));
    await act(async () => { await result.current.onLock({ ...mockVaultRoot, vaultLocked: false }); });
    expect(lockVault).toHaveBeenCalledWith(7);
    expect(callbacks.onNotice).toHaveBeenCalledWith('Locked "secret"');
    expect(callbacks.onVisibilityChanged).toHaveBeenCalled();

    vi.mocked(lockVault).mockRejectedValue(new Error("in use"));
    await act(async () => { await result.current.onLock({ ...mockVaultRoot, vaultLocked: false }); });
    expect(callbacks.onError).toHaveBeenCalledWith("in use");
  });
});
