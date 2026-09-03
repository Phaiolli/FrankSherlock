import { describe, it, expect, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import AddFolderModal, { validateVaultPassword, vaultProgressPercent } from "../../components/modals/AddFolderModal";

const supported = { supported: true, reason: null };

function renderModal(overrides: Partial<React.ComponentProps<typeof AddFolderModal>> = {}) {
  const props = {
    folderPath: "/home/user/Secret Stuff",
    vaultSupport: supported,
    busy: false,
    onCancel: vi.fn(),
    onAddPlain: vi.fn(),
    onCreateVault: vi.fn().mockResolvedValue(null),
    ...overrides,
  };
  render(<AddFolderModal {...props} />);
  return props;
}

describe("validateVaultPassword", () => {
  it("rejects short and mismatched passwords", () => {
    expect(validateVaultPassword("abc", "abc")).toMatch(/at least/);
    expect(validateVaultPassword("abcd", "abce")).toMatch(/match/);
    expect(validateVaultPassword("abcd", "abcd")).toBeNull();
  });
});

describe("AddFolderModal", () => {
  it("shows the folder path and adds a plain folder by default", async () => {
    const props = renderModal();
    expect(screen.getByText("/home/user/Secret Stuff")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Add folder" }));
    expect(props.onAddPlain).toHaveBeenCalled();
    expect(props.onCreateVault).not.toHaveBeenCalled();
  });

  it("reveals password fields and warning when secret is checked", async () => {
    renderModal();
    expect(screen.queryByLabelText("Vault password")).not.toBeInTheDocument();
    await userEvent.click(screen.getByLabelText("Secret folder"));
    expect(screen.getByLabelText("Vault password")).toBeInTheDocument();
    expect(screen.getByLabelText("Confirm vault password")).toBeInTheDocument();
    expect(screen.getByText(/cannot be recovered/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create secret folder" })).toBeInTheDocument();
  });

  it("validates password before creating the vault", async () => {
    const props = renderModal();
    await userEvent.click(screen.getByLabelText("Secret folder"));
    await userEvent.type(screen.getByLabelText("Vault password"), "abcd");
    await userEvent.type(screen.getByLabelText("Confirm vault password"), "abce");
    await userEvent.click(screen.getByRole("button", { name: "Create secret folder" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Passwords do not match");
    expect(props.onCreateVault).not.toHaveBeenCalled();
  });

  it("requires the acknowledgement checkbox", async () => {
    const props = renderModal();
    await userEvent.click(screen.getByLabelText("Secret folder"));
    await userEvent.type(screen.getByLabelText("Vault password"), "hunter2");
    await userEvent.type(screen.getByLabelText("Confirm vault password"), "hunter2");
    await userEvent.click(screen.getByRole("button", { name: "Create secret folder" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/confirm you understand/);
    expect(props.onCreateVault).not.toHaveBeenCalled();
  });

  it("creates the vault with the password and shows backend errors inline", async () => {
    const props = renderModal({ onCreateVault: vi.fn().mockResolvedValue("gocryptfs exploded") });
    await userEvent.click(screen.getByLabelText("Secret folder"));
    await userEvent.type(screen.getByLabelText("Vault password"), "hunter2");
    await userEvent.type(screen.getByLabelText("Confirm vault password"), "hunter2");
    await userEvent.click(screen.getByLabelText("I understand the password cannot be recovered"));
    await userEvent.click(screen.getByRole("button", { name: "Create secret folder" }));
    expect(props.onCreateVault).toHaveBeenCalledWith("hunter2");
    expect(await screen.findByRole("alert")).toHaveTextContent("gocryptfs exploded");
  });

  it("disables the secret option and explains why when unsupported", () => {
    renderModal({ vaultSupport: { supported: false, reason: "gocryptfs is not installed" } });
    expect(screen.getByLabelText("Secret folder")).toBeDisabled();
    expect(screen.getByText("gocryptfs is not installed")).toBeInTheDocument();
  });

  it("switches to reopen mode when an encrypted store already exists", async () => {
    const onAttachVault = vi.fn().mockResolvedValue(null);
    const props = renderModal({
      folderPath: "/home/user/.Secret Stuff.vault",
      probe: { attachable: true, mountPoint: "/home/user/Secret Stuff", cipherDir: "/home/user/.Secret Stuff.vault" },
      onAttachVault,
    });
    expect(screen.getByRole("heading", { name: "Reopen secret folder" })).toBeInTheDocument();
    expect(screen.queryByLabelText("Secret folder")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Reopen secret folder" }));
    expect(screen.getByRole("alert")).toHaveTextContent("Enter the vault password");
    await userEvent.type(screen.getByLabelText("Vault password"), "hunter2{Enter}");
    expect(onAttachVault).toHaveBeenCalledWith("hunter2");
    expect(props.onAddPlain).not.toHaveBeenCalled();
    expect(props.onCreateVault).not.toHaveBeenCalled();
  });

  it("shows attach errors inline and clears the password", async () => {
    renderModal({
      probe: { attachable: true, mountPoint: "/home/user/Secret Stuff", cipherDir: "/home/user/.Secret Stuff.vault" },
      onAttachVault: vi.fn().mockResolvedValue("Incorrect password"),
    });
    await userEvent.type(screen.getByLabelText("Vault password"), "nope{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("Incorrect password");
    expect(screen.getByLabelText("Vault password")).toHaveValue("");
  });

  it("calls onCancel", async () => {
    const props = renderModal();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(props.onCancel).toHaveBeenCalled();
  });
});

describe("vaultProgressPercent", () => {
  it("prefers bytes, falls back to files and is null when nothing is known", () => {
    expect(vaultProgressPercent({ phase: "encrypting", processedFiles: 1, totalFiles: 4, processedBytes: 50, totalBytes: 200 })).toBe(25);
    expect(vaultProgressPercent({ phase: "encrypting", processedFiles: 1, totalFiles: 4, processedBytes: 0, totalBytes: 0 })).toBe(25);
    expect(vaultProgressPercent({ phase: "preparing", processedFiles: 0, totalFiles: 0, processedBytes: 0, totalBytes: 0 })).toBeNull();
  });
});

describe("AddFolderModal progress", () => {
  it("shows the phase, counts and sizes while encrypting", () => {
    renderModal({
      busy: true,
      progress: { phase: "encrypting", processedFiles: 3, totalFiles: 10, processedBytes: 2048, totalBytes: 8192 },
    });
    expect(screen.getByRole("status")).toHaveTextContent("Encrypting files...");
    expect(screen.getByRole("status")).toHaveTextContent("3 / 10 files");
    expect(screen.getByRole("status")).toHaveTextContent("2.0 KB of 8.0 KB");
  });

  it("names the other phases", () => {
    renderModal({ busy: true, progress: { phase: "verifying", processedFiles: 10, totalFiles: 10, processedBytes: 8192, totalBytes: 8192 } });
    expect(screen.getByRole("status")).toHaveTextContent("Verifying the copy...");
  });

  it("hides the bar when not busy or with no progress yet", () => {
    renderModal({ busy: false, progress: { phase: "encrypting", processedFiles: 1, totalFiles: 2, processedBytes: 1, totalBytes: 2 } });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    cleanup();
    renderModal({ busy: true, progress: null });
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });
});
