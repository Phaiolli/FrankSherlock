import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import PickLocationModal from "../../components/modals/PickLocationModal";
import type { VolumeInfo } from "../../types";

const volumes: VolumeInfo[] = [
  { name: "Home", path: "/home/user", kind: "home" },
  { name: "arquivos", path: "/mnt/arquivos", kind: "drive" },
  { name: "Filesystem root", path: "/", kind: "root" },
];

function setup(overrides: Partial<React.ComponentProps<typeof PickLocationModal>> = {}) {
  const props = {
    volumes,
    loading: false,
    error: null,
    onCancel: vi.fn(),
    onBrowse: vi.fn(),
    onUsePath: vi.fn(),
    ...overrides,
  };
  render(<PickLocationModal {...props} />);
  return props;
}

describe("PickLocationModal", () => {
  it("lists every mounted disk with its path", () => {
    setup();
    expect(screen.getByText("arquivos")).toBeInTheDocument();
    expect(screen.getByText("/mnt/arquivos")).toBeInTheDocument();
    expect(screen.getByText("Filesystem root")).toBeInTheDocument();
  });

  it("browses from the disk that was clicked", async () => {
    const user = userEvent.setup();
    const props = setup();
    await user.click(screen.getByText("arquivos"));
    expect(props.onBrowse).toHaveBeenCalledWith("/mnt/arquivos");
  });

  it("browses without a start path from the Browse button", async () => {
    const user = userEvent.setup();
    const props = setup();
    await user.click(screen.getByText("Browse..."));
    expect(props.onBrowse).toHaveBeenCalledWith();
  });

  it("submits a typed path with the button and with Enter", async () => {
    const user = userEvent.setup();
    const props = setup();
    const input = screen.getByLabelText("Folder path");
    await user.type(input, "/mnt/trabalho/fotos");
    await user.click(screen.getByText("Use path"));
    expect(props.onUsePath).toHaveBeenCalledWith("/mnt/trabalho/fotos");
    await user.type(input, "{Enter}");
    expect(props.onUsePath).toHaveBeenCalledTimes(2);
  });

  it("disables Use path while the field is blank", async () => {
    const user = userEvent.setup();
    setup();
    expect(screen.getByText("Use path")).toBeDisabled();
    await user.type(screen.getByLabelText("Folder path"), "   ");
    expect(screen.getByText("Use path")).toBeDisabled();
  });

  it("shows a path error inline", () => {
    setup({ error: "Path not found: /mnt/nope" });
    expect(screen.getByText("Path not found: /mnt/nope")).toBeInTheDocument();
  });

  it("shows a loading hint while disks are being listed", () => {
    setup({ volumes: [], loading: true });
    expect(screen.getByText("Looking for disks...")).toBeInTheDocument();
  });

  it("cancels from the button and from Escape", async () => {
    const user = userEvent.setup();
    const props = setup();
    await user.click(screen.getByText("Cancel"));
    expect(props.onCancel).toHaveBeenCalledOnce();
    vi.mocked(props.onCancel).mockClear();
    await user.type(screen.getByLabelText("Folder path"), "{Escape}");
    expect(props.onCancel).toHaveBeenCalled();
  });
});
