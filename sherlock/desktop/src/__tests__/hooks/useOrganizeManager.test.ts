import { describe, it, expect, vi, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useOrganizeManager } from "../../hooks/useOrganizeManager";
import { organizeRootByPeople } from "../../api";
import { mockRoot } from "../fixtures";

vi.mock("../../api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../api")>();
  return { ...actual, organizeRootByPeople: vi.fn() };
});

describe("useOrganizeManager", () => {
  const callbacks = {
    onNotice: vi.fn(),
    onError: vi.fn(),
    onChanged: vi.fn().mockResolvedValue(undefined),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(organizeRootByPeople).mockResolvedValue({ moved: 3, copied: 1, skipped: 2, people: 2, errors: [] });
  });

  it("runs the organisation for the target and reports a summary", async () => {
    const { result } = renderHook(() => useOrganizeManager(callbacks));
    act(() => result.current.setOrganizeTarget(mockRoot));
    await act(async () => { await result.current.onConfirm(); });
    expect(organizeRootByPeople).toHaveBeenCalledWith(1);
    expect(callbacks.onNotice).toHaveBeenCalledWith('Organized "photos": 3 moved, 1 copied, 2 people, 2 already in place');
    expect(callbacks.onChanged).toHaveBeenCalled();
    expect(result.current.organizeTarget).toBeNull();
  });

  it("surfaces per-file errors and backend failures", async () => {
    vi.mocked(organizeRootByPeople).mockResolvedValue({ moved: 1, copied: 0, skipped: 0, people: 1, errors: ["x.jpg: move failed"] });
    const { result } = renderHook(() => useOrganizeManager(callbacks));
    act(() => result.current.setOrganizeTarget(mockRoot));
    await act(async () => { await result.current.onConfirm(); });
    expect(callbacks.onError).toHaveBeenCalledWith(expect.stringContaining("x.jpg: move failed"));

    vi.mocked(organizeRootByPeople).mockRejectedValue("Wait for the running scan to finish before organizing");
    act(() => result.current.setOrganizeTarget(mockRoot));
    await act(async () => { await result.current.onConfirm(); });
    expect(callbacks.onError).toHaveBeenCalledWith("Wait for the running scan to finish before organizing");
    expect(result.current.organizeTarget).toEqual(mockRoot);
  });

  it("does nothing without a target", async () => {
    const { result } = renderHook(() => useOrganizeManager(callbacks));
    await act(async () => { await result.current.onConfirm(); });
    expect(organizeRootByPeople).not.toHaveBeenCalled();
  });
});
