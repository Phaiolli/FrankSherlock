import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import UnlockVaultModal from "../../components/modals/UnlockVaultModal";
import { mockVaultRoot } from "../fixtures";

describe("UnlockVaultModal", () => {
  it("submits the password on Enter and closes on success", async () => {
    const onUnlock = vi.fn().mockResolvedValue(null);
    render(<UnlockVaultModal root={mockVaultRoot} busy={false} onCancel={vi.fn()} onUnlock={onUnlock} />);
    expect(screen.getByText("secret")).toBeInTheDocument();
    await userEvent.type(screen.getByLabelText("Vault password"), "hunter2{Enter}");
    expect(onUnlock).toHaveBeenCalledWith("hunter2");
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });

  it("shows the error returned by the backend and clears the field", async () => {
    const onUnlock = vi.fn().mockResolvedValue("Incorrect password");
    render(<UnlockVaultModal root={mockVaultRoot} busy={false} onCancel={vi.fn()} onUnlock={onUnlock} />);
    await userEvent.type(screen.getByLabelText("Vault password"), "nope");
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Incorrect password");
    expect(screen.getByLabelText("Vault password")).toHaveValue("");
  });

  it("refuses an empty password without calling the backend", async () => {
    const onUnlock = vi.fn();
    render(<UnlockVaultModal root={mockVaultRoot} busy={false} onCancel={vi.fn()} onUnlock={onUnlock} />);
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Enter the password");
    expect(onUnlock).not.toHaveBeenCalled();
  });

  it("disables controls while busy", () => {
    render(<UnlockVaultModal root={mockVaultRoot} busy onCancel={vi.fn()} onUnlock={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Unlocking..." })).toBeDisabled();
    expect(screen.getByLabelText("Vault password")).toBeDisabled();
  });
});
