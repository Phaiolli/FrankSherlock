import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import RootCard from "../../components/Sidebar/RootCard";
import { mockRoot as sampleRoot, mockRunningScan, mockVaultRoot } from "../fixtures";

describe("RootCard", () => {
  it("renders root name and file count", () => {
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} />
    );
    expect(screen.getByText("photos")).toBeInTheDocument();
    expect(screen.getByText("42 files")).toBeInTheDocument();
  });

  it("applies selected class when selected", () => {
    const { container } = render(
      <RootCard root={sampleRoot} isSelected scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} />
    );
    expect(container.querySelector(".root-card.selected")).not.toBeNull();
  });

  it("calls onSelect when clicked", async () => {
    const onSelect = vi.fn();
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={onSelect} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} />
    );
    await userEvent.click(screen.getByText("photos"));
    expect(onSelect).toHaveBeenCalled();
  });

  it("calls onDelete when delete button clicked", async () => {
    const onDelete = vi.fn();
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={onDelete} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} />
    );
    await userEvent.click(screen.getByLabelText("Remove photos"));
    expect(onDelete).toHaveBeenCalled();
  });

  it("hides delete button in readOnly mode", () => {
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} />
    );
    expect(screen.queryByLabelText("Remove photos")).not.toBeInTheDocument();
  });

  it("shows scan progress with stats when scan is running (classifying phase)", () => {
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={mockRunningScan} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onCancelScan={vi.fn()} />
    );
    expect(screen.getByText("Classifying 50/100")).toBeInTheDocument();
  });

  it("shows scan progress with stats when scan is thumbnailing", () => {
    const thumbnailingScan = { ...mockRunningScan, phase: "thumbnailing" as const };
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={thumbnailingScan} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onCancelScan={vi.fn()} />
    );
    expect(screen.getByText("Thumbnailing 50/100")).toBeInTheDocument();
    expect(screen.getByText(/\+10 new, 5 mod, 2 moved/)).toBeInTheDocument();
  });

  it("shows pause button for running scan", () => {
    const onCancelScan = vi.fn();
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={mockRunningScan} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onCancelScan={onCancelScan} />
    );
    expect(screen.getByText("Pause")).toBeInTheDocument();
  });

  it("hides pause button in readOnly mode", () => {
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={mockRunningScan} readOnly onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onCancelScan={vi.fn()} />
    );
    expect(screen.queryByText("Pause")).not.toBeInTheDocument();
  });

  it("shows resume button for interrupted scan", () => {
    const interruptedScan = { ...mockRunningScan, status: "interrupted" as const };
    const onResumeScan = vi.fn();
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={interruptedScan} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onResumeScan={onResumeScan} />
    );
    expect(screen.getByText("Scan interrupted")).toBeInTheDocument();
    expect(screen.getByText("Resume")).toBeInTheDocument();
  });

  it("hides resume button in readOnly mode", () => {
    const interruptedScan = { ...mockRunningScan, status: "interrupted" as const };
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={interruptedScan} readOnly onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onResumeScan={vi.fn()} />
    );
    expect(screen.queryByText("Resume")).not.toBeInTheDocument();
  });

  it("shows Refresh Metadata in context menu", async () => {
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} />
    );
    const card = screen.getByText("photos").closest(".root-card")!;
    await userEvent.pointer({ keys: "[MouseRight]", target: card });
    expect(screen.getByRole("menuitem", { name: "Refresh Metadata" })).toBeInTheDocument();
  });

  it("calls onRefresh from context menu", async () => {
    const onRefresh = vi.fn();
    render(
      <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={onRefresh} onCopyPath={vi.fn()} />
    );
    const card = screen.getByText("photos").closest(".root-card")!;
    await userEvent.pointer({ keys: "[MouseRight]", target: card });
    await userEvent.click(screen.getByRole("menuitem", { name: "Refresh Metadata" }));
    expect(onRefresh).toHaveBeenCalled();
  });
  describe("secret folders", () => {
    it("shows a Locked badge and Unlock button for a locked vault", async () => {
      const onUnlockVault = vi.fn();
      render(
        <RootCard root={mockVaultRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onUnlockVault={onUnlockVault} onLockVault={vi.fn()} />
      );
      expect(screen.getByText("Locked")).toBeInTheDocument();
      expect(screen.getByLabelText("Locked secret folder")).toBeInTheDocument();
      await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
      expect(onUnlockVault).toHaveBeenCalled();
      expect(screen.queryByRole("button", { name: "Lock" })).not.toBeInTheDocument();
    });

    it("shows Lock button for an unlocked vault and hides it while scanning", () => {
      const unlocked = { ...mockVaultRoot, vaultLocked: false };
      const { rerender } = render(
        <RootCard root={unlocked} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onUnlockVault={vi.fn()} onLockVault={vi.fn()} />
      );
      expect(screen.getByText("Unlocked")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Lock" })).toBeInTheDocument();
      rerender(
        <RootCard root={unlocked} isSelected={false} scan={mockRunningScan} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onUnlockVault={vi.fn()} onLockVault={vi.fn()} />
      );
      expect(screen.queryByRole("button", { name: "Lock" })).not.toBeInTheDocument();
    });

    it("context menu of a locked vault hides scan actions and offers Unlock", async () => {
      const onUnlockVault = vi.fn();
      render(
        <RootCard root={mockVaultRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onDetectFaces={vi.fn()} onUnlockVault={onUnlockVault} onLockVault={vi.fn()} />
      );
      await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByText("secret") });
      expect(screen.queryByRole("menuitem", { name: "Rescan" })).not.toBeInTheDocument();
      expect(screen.queryByRole("menuitem", { name: "Detect Faces" })).not.toBeInTheDocument();
      await userEvent.click(screen.getByRole("menuitem", { name: "Unlock" }));
      expect(onUnlockVault).toHaveBeenCalled();
    });

    it("context menu offers Organize by People when a handler is given", async () => {
      const onOrganize = vi.fn();
      render(
        <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onOrganize={onOrganize} />
      );
      await userEvent.pointer({ keys: "[MouseRight]", target: screen.getByText("photos") });
      await userEvent.click(screen.getByRole("menuitem", { name: "Organize by People" }));
      expect(onOrganize).toHaveBeenCalled();
    });

    it("never shows vault buttons for a normal folder", () => {
      render(
        <RootCard root={sampleRoot} isSelected={false} scan={undefined} readOnly={false} onSelect={vi.fn()} onDelete={vi.fn()} onRescan={vi.fn()} onRefresh={vi.fn()} onCopyPath={vi.fn()} onUnlockVault={vi.fn()} onLockVault={vi.fn()} />
      );
      expect(screen.queryByRole("button", { name: "Lock" })).not.toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Unlock" })).not.toBeInTheDocument();
    });
  });
});
