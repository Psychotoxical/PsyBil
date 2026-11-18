import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { DraftInput } from "../ipc/bindings/DraftInput";
import { createDraft, draftTotals, saveDraft } from "../ipc/invoices";
import { flushDraft, useDraft } from "./draft";

vi.mock("../ipc/invoices", () => ({
  createDraft: vi.fn(),
  saveDraft: vi.fn().mockResolvedValue(undefined),
  draftTotals: vi
    .fn()
    .mockResolvedValue({ line_amounts: [], groups: [], net: 0, tax: 0, gross: 0 }),
}));

const BASE: DraftInput = {
  brand_id: "b1",
  customer_id: null,
  invoice_date: "2026-11-18",
  service_date: { type: "same_as_invoice" },
  payment_days: 14,
  price_mode: "net",
  language: "de",
  intro_text: "",
  outro_text: "",
  notes: "",
  lines: [],
};

describe("draft store", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.useFakeTimers();
    vi.mocked(createDraft).mockResolvedValue("d1");
    useDraft.getState().open(null, null);
  });
  afterEach(() => vi.useRealTimers());

  it("typing_fast_creates_one_draft_then_saves", async () => {
    const { change } = useDraft.getState();
    for (let i = 0; i < 5; i++) {
      change({ ...BASE, notes: `n${i}` });
      await vi.advanceTimersByTimeAsync(40);
    }
    await vi.advanceTimersByTimeAsync(1000);
    expect(createDraft).toHaveBeenCalledTimes(1);
    expect(createDraft).toHaveBeenCalledWith({ ...BASE, notes: "n0" });
    expect(saveDraft).toHaveBeenCalledTimes(1);
    expect(saveDraft).toHaveBeenCalledWith("d1", { ...BASE, notes: "n4" });
    expect(useDraft.getState().id).toBe("d1");
    expect(draftTotals).toHaveBeenCalledWith("d1");
  });

  it("flush_on_close_writes_pending_change", async () => {
    useDraft.getState().open("d1", BASE);
    useDraft.getState().change({ ...BASE, notes: "spät" });
    expect(saveDraft).not.toHaveBeenCalled();
    await flushDraft();
    expect(saveDraft).toHaveBeenCalledWith("d1", { ...BASE, notes: "spät" });
    await vi.advanceTimersByTimeAsync(1000);
    expect(saveDraft).toHaveBeenCalledTimes(1);
  });

  it("totals_failure_is_kept_for_the_window", async () => {
    vi.mocked(draftTotals).mockRejectedValueOnce({
      code: "too_many_decimals",
      message: "too many decimal places",
      issues: [],
    });
    useDraft.getState().open("d1", BASE);
    await vi.advanceTimersByTimeAsync(10);
    expect(useDraft.getState().totals).toBeNull();
    expect(useDraft.getState().totalsError?.code).toBe("too_many_decimals");
  });

  it("moving_and_removing_lines_moves_their_amounts", () => {
    const line = (description: string) => ({
      kind: "free" as const,
      product_id: null,
      description,
      quantity: "1",
      unit: "",
      unit_price: "1",
      tax: { mode: "follow_goods" as const },
    });
    const s = useDraft.getState();
    s.open("d1", { ...BASE, lines: [line("A"), line("B")] });
    useDraft.setState({
      totals: { line_amounts: [100, 200], groups: [], net: 300, tax: 0, gross: 300 },
    });
    s.moveLine(1, -1);
    expect(useDraft.getState().totals?.line_amounts).toEqual([200, 100]);
    s.removeLine(0);
    expect(useDraft.getState().totals?.line_amounts).toEqual([100]);
  });

  it("lines_move_with_their_keys", () => {
    const line = (description: string) => ({
      kind: "free" as const,
      product_id: null,
      description,
      quantity: "1",
      unit: "",
      unit_price: "1",
      tax: { mode: "follow_goods" as const },
    });
    const s = useDraft.getState();
    s.open("d1", BASE);
    s.addLine(line("A"));
    s.addLine(line("B"));
    const [ka, kb] = useDraft.getState().keys;
    s.moveLine(1, -1);
    expect(useDraft.getState().input?.lines.map((l) => l.description)).toEqual(["B", "A"]);
    expect(useDraft.getState().keys).toEqual([kb, ka]);
    s.removeLine(0);
    expect(useDraft.getState().input?.lines.map((l) => l.description)).toEqual(["A"]);
    expect(useDraft.getState().keys).toEqual([ka]);
  });
});
