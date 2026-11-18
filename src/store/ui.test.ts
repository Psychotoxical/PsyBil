import { beforeEach, describe, expect, it } from "vitest";
import { useUi } from "./ui";

describe("ui store", () => {
  beforeEach(() => {
    useUi.setState({
      route: { page: "customers" },
      toasts: [],
      unsaved: false,
      pendingRoute: null,
    });
  });

  it("the_same_message_twice_shows_once", () => {
    const { toast } = useUi.getState();
    toast("error", "Die Daten sind gerade belegt.");
    toast("error", "Die Daten sind gerade belegt.");
    toast("info", "Gespeichert.");
    expect(useUi.getState().toasts.map((t) => t.text)).toEqual([
      "Die Daten sind gerade belegt.",
      "Gespeichert.",
    ]);
  });

  it("leaving_an_unsaved_form_asks_first", () => {
    const { setUnsaved, navigate } = useUi.getState();
    setUnsaved(true);
    navigate({ page: "invoices" });
    expect(useUi.getState().route).toEqual({ page: "customers" });
    expect(useUi.getState().pendingRoute).toEqual({ page: "invoices" });

    useUi.getState().stay();
    expect(useUi.getState().pendingRoute).toBeNull();
    expect(useUi.getState().route).toEqual({ page: "customers" });

    navigate({ page: "invoices" });
    useUi.getState().leave();
    expect(useUi.getState().route).toEqual({ page: "invoices" });
    expect(useUi.getState().unsaved).toBe(false);
  });
});
