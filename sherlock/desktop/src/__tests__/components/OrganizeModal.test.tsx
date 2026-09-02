import { describe, it, expect, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import OrganizeModal from "../../components/modals/OrganizeModal";
import { mockRoot } from "../fixtures";

describe("OrganizeModal", () => {
  it("explains the operation and confirms", async () => {
    const onConfirm = vi.fn();
    render(<OrganizeModal root={mockRoot} busy={false} onCancel={vi.fn()} onConfirm={onConfirm} />);
    expect(screen.getByText(/Organize by people\?/)).toBeInTheDocument();
    expect(screen.getByText("/home/user/photos")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Organize" }));
    expect(onConfirm).toHaveBeenCalled();
  });

  it("cancels and disables while busy", async () => {
    const onCancel = vi.fn();
    const { rerender } = render(<OrganizeModal root={mockRoot} busy={false} onCancel={onCancel} onConfirm={vi.fn()} />);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onCancel).toHaveBeenCalled();
    rerender(<OrganizeModal root={mockRoot} busy onCancel={onCancel} onConfirm={vi.fn()} />);
    expect(screen.getByRole("button", { name: "Organizing..." })).toBeDisabled();
  });
});
