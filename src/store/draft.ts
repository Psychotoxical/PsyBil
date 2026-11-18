import { create } from "zustand";
import type { DraftInput } from "../ipc/bindings/DraftInput";
import type { LineDraft } from "../ipc/bindings/LineDraft";
import type { Totals } from "../ipc/bindings/Totals";
import { type CmdError, toCmdError } from "../ipc/errors";
import { createDraft, draftTotals, saveDraft } from "../ipc/invoices";

/** Saving waits this long after the last change. */
export const SAVE_DELAY = 600;

export type SaveState = "idle" | "pending" | "saving" | "saved" | "error";

type State = {
  /** The draft in the core; null until the first input created it. */
  id: string | null;
  input: DraftInput | null;
  /** One stable key per line, moved along with it. */
  keys: number[];
  /** From `draft_totals` after each save; the window never adds up amounts itself. */
  totals: Totals | null;
  /** Why the core could not add up the draft; shown instead of an old total. */
  totalsError: CmdError | null;
  save: SaveState;
  error: CmdError | null;
  /** Counts finished saves; the preview follows it. */
  savedCount: number;
  /** Starts a new draft (null) or shows a stored one. */
  open: (id: string | null, input: DraftInput | null) => void;
  /** The first change creates the draft, later ones are saved after a pause. */
  change: (input: DraftInput) => void;
  addLine: (line: LineDraft) => void;
  updateLine: (index: number, patch: Partial<LineDraft>) => void;
  moveLine: (index: number, by: -1 | 1) => void;
  removeLine: (index: number) => void;
  /** Writes what is still waiting, at once. */
  flush: () => Promise<void>;
};

let timer: ReturnType<typeof setTimeout> | undefined;
let creating: Promise<void> | null = null;
let queue: Promise<void> = Promise.resolve();
let lastSaved: DraftInput | null = null;
let generation = 0;
let nextKey = 1;

export const useDraft = create<State>((set, get) => {
  const loadTotals = (id: string, gen: number) => {
    draftTotals(id)
      .then((totals) => {
        if (gen === generation) set({ totals, totalsError: null });
      })
      .catch((error) => {
        if (gen === generation) set({ totals: null, totalsError: toCmdError(error) });
      });
  };

  const saved = (id: string, gen: number) => {
    set({ save: "saved", error: null, savedCount: get().savedCount + 1 });
    loadTotals(id, gen);
  };

  const failed = (error: unknown) => set({ save: "error", error: toCmdError(error) });

  /** Saves the latest input if it differs from what the core has. */
  const write = async () => {
    clearTimeout(timer);
    timer = undefined;
    const gen = generation;
    if (creating) await creating;
    const { id, input } = get();
    if (gen !== generation || !id || !input || input === lastSaved) return;
    set({ save: "saving" });
    try {
      await saveDraft(id, input);
      if (gen !== generation) return;
      lastSaved = input;
      saved(id, gen);
    } catch (error) {
      if (gen === generation) failed(error);
    }
  };

  const enqueue = () => {
    queue = queue.then(write, write);
    return queue;
  };

  const schedule = () => {
    clearTimeout(timer);
    timer = setTimeout(() => void enqueue(), SAVE_DELAY);
  };

  const create_ = (input: DraftInput) => {
    const gen = generation;
    set({ save: "saving" });
    creating = createDraft(input)
      .then((id) => {
        if (gen !== generation) return;
        lastSaved = input;
        set({ id });
        saved(id, gen);
      })
      .catch((error) => {
        if (gen === generation) failed(error);
      })
      .finally(() => {
        if (gen === generation) creating = null;
      });
  };

  /** Changes the lines; keys and the core's line amounts move with them until the next total. */
  const lines = (fn: (lines: LineDraft[], keys: number[], amounts: number[]) => void) => {
    const { input, keys, totals } = get();
    if (!input) return;
    const nextLines = [...input.lines];
    const nextKeys = [...keys];
    const amounts = [...(totals?.line_amounts ?? [])];
    fn(nextLines, nextKeys, amounts);
    set({ keys: nextKeys, totals: totals && { ...totals, line_amounts: amounts } });
    get().change({ ...input, lines: nextLines });
  };

  return {
    id: null,
    input: null,
    keys: [],
    totals: null,
    totalsError: null,
    save: "idle",
    error: null,
    savedCount: 0,

    open: (id, input) => {
      // A change still waiting for the old draft is written before it goes.
      const old = get();
      if (timer !== undefined && old.id && old.input && old.input !== lastSaved) {
        const [oldId, oldInput] = [old.id, old.input];
        queue = queue.then(() => saveDraft(oldId, oldInput).catch(() => {}));
      }
      clearTimeout(timer);
      timer = undefined;
      generation += 1;
      creating = null;
      lastSaved = input;
      set({
        id,
        input,
        keys: input ? input.lines.map(() => nextKey++) : [],
        totals: null,
        totalsError: null,
        save: id ? "saved" : "idle",
        error: null,
        savedCount: 0,
      });
      if (id) loadTotals(id, generation);
    },

    change: (input) => {
      set({ input, save: "pending" });
      if (!get().id && !creating) create_(input);
      else schedule();
    },

    addLine: (line) =>
      lines((all, keys) => {
        all.push(line);
        keys.push(nextKey++);
      }),
    updateLine: (index, patch) =>
      lines((all) => {
        const current = all[index];
        if (current) all[index] = { ...current, ...patch };
      }),
    moveLine: (index, by) =>
      lines((all, keys, amounts) => {
        const to = index + by;
        if (to < 0 || to >= all.length) return;
        [all[index], all[to]] = [all[to] as LineDraft, all[index] as LineDraft];
        [keys[index], keys[to]] = [keys[to] as number, keys[index] as number];
        if (index < amounts.length && to < amounts.length)
          [amounts[index], amounts[to]] = [amounts[to] as number, amounts[index] as number];
      }),
    removeLine: (index) =>
      lines((all, keys, amounts) => {
        all.splice(index, 1);
        keys.splice(index, 1);
        amounts.splice(index, 1);
      }),

    flush: () => (timer !== undefined || creating ? enqueue() : queue),
  };
});

/** Writes a waiting change now, e.g. before the window closes. */
export const flushDraft = () => useDraft.getState().flush();
