import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";
import { describe, expect, it } from "vitest";
import { ConfirmDialog } from "./ConfirmDialog";
import { Dialog } from "./Dialog";

function Harness() {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>
        Öffnen
      </button>
      {open && (
        <Dialog title="Frage" onClose={() => setOpen(false)}>
          <button type="button">Erster</button>
          <button type="button">Letzter</button>
        </Dialog>
      )}
    </>
  );
}

describe("Dialog", () => {
  it("dialog_traps_focus_closes_on_escape_and_restores_focus", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Öffnen" });
    await user.click(opener);
    const dialog = screen.getByRole("dialog", { name: "Frage" });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(screen.getByRole("button", { name: "Erster" })).toHaveFocus();
    await user.tab();
    await user.tab();
    // Past "Letzter" and the close button the focus wraps back inside.
    expect(dialog.contains(document.activeElement)).toBe(true);
    await user.tab({ shift: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(opener).toHaveFocus();
  });

  it("confirm_dialog_keeps_focus_inside_and_starts_on_the_safe_button", async () => {
    const user = userEvent.setup();
    let closed = 0;
    render(
      <>
        <button type="button">Dahinter</button>
        <ConfirmDialog
          title="Löschen?"
          text="Das lässt sich nicht rückgängig machen."
          confirmLabel="Löschen"
          danger
          onConfirm={() => {}}
          onCancel={() => {
            closed += 1;
          }}
        />
      </>,
    );
    const dialog = screen.getByRole("dialog");
    expect(screen.getByRole("button", { name: "Abbrechen" })).toHaveFocus();
    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(dialog.contains(document.activeElement)).toBe(true);
    await user.keyboard("{Shift>}{Tab}{/Shift}{Shift>}{Tab}{/Shift}");
    expect(dialog.contains(document.activeElement)).toBe(true);
    await user.keyboard("{Escape}");
    expect(closed).toBe(1);
  });
});
