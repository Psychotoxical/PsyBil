import "./assistant.css";
import { CircleAlert, RotateCcw, Trash2 } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents } from "../../format";
import type { Brand } from "../../ipc/bindings/Brand";
import type { Business } from "../../ipc/bindings/Business";
import type { Customer } from "../../ipc/bindings/Customer";
import type { DraftInput } from "../../ipc/bindings/DraftInput";
import type { Invoice } from "../../ipc/bindings/Invoice";
import type { Product } from "../../ipc/bindings/Product";
import { messageFor, toCmdError } from "../../ipc/errors";
import { deleteDraft, loadInvoice } from "../../ipc/invoices";
import type { IssueTarget } from "../../ipc/issueTexts";
import { listBrands, listBusinesses, listCustomers, listProducts } from "../../ipc/masterdata";
import { ConfirmDialog } from "../../shell/ConfirmDialog";
import { SidePanel } from "../../shell/SidePanel";
import { StepTiles } from "../../shell/StepTiles";
import { flushDraft, useDraft } from "../../store/draft";
import { currentSettings, useSettings } from "../../store/settings";
import { useUi } from "../../store/ui";
import { Preview } from "./Preview";
import { StepCustomer } from "./StepCustomer";
import { StepDates } from "./StepDates";
import { StepIssue } from "./StepIssue";
import { StepLines } from "./StepLines";
import { StepSender } from "./StepSender";

const REVIEW = 4;

/** The first step that still needs an answer when a stored draft is opened. */
function firstOpenStep(input: DraftInput): number {
  if (!input.customer_id) return 1;
  if (input.lines.length === 0) return 2;
  return 3;
}

function toInput(invoice: Invoice): DraftInput {
  return {
    brand_id: invoice.brand_id,
    customer_id: invoice.customer_id,
    invoice_date: invoice.invoice_date,
    service_date: invoice.service_date,
    payment_days: invoice.payment_days,
    price_mode: invoice.price_mode,
    language: invoice.language,
    intro_text: invoice.intro_text,
    outro_text: invoice.outro_text,
    notes: invoice.notes,
    lines: invoice.lines,
  };
}

/** "New invoice": five steps with the real invoice growing beside them. */
export function Assistant({ id }: { id?: string }) {
  const { t, i18n } = useTranslation();
  const navigate = useUi((s) => s.navigate);
  const toast = useUi((s) => s.toast);
  const draftId = useDraft((s) => s.id);
  const input = useDraft((s) => s.input);
  const totals = useDraft((s) => s.totals);
  const totalsError = useDraft((s) => s.totalsError);
  const save = useDraft((s) => s.save);
  const saveError = useDraft((s) => s.error);
  const previewOpen = useSettings((s) => s.settings?.preview_open ?? true);
  const updateSettings = useSettings((s) => s.update);

  const [step, setStep] = useState(0);
  const [datesSeen, setDatesSeen] = useState(false);
  const [fresh, setFresh] = useState(!id);
  const [confirm, setConfirm] = useState(false);
  const [businesses, setBusinesses] = useState<Business[]>([]);
  const [brands, setBrands] = useState<Brand[]>([]);
  const [customers, setCustomers] = useState<Customer[]>([]);
  const [products, setProducts] = useState<Product[]>([]);
  const mounted = useRef(false);
  const opened = useRef(false);
  const focusCell = useRef<{ row: number; col: string } | null>(null);

  useEffect(() => {
    (async () => {
      const all = await listBusinesses(false);
      const lists = await Promise.all(all.map((b) => listBrands(b.id)));
      setBusinesses(all);
      setBrands(lists.flat().filter((b) => !b.archived));
    })().catch((e) => toast("error", messageFor(toCmdError(e), t)));
    listCustomers(false)
      .then(setCustomers)
      .catch(() => {});
  }, [toast, t]);

  const reloadCustomers = useCallback(async () => {
    const all = await listCustomers(false);
    setCustomers(all);
    return all;
  }, []);

  const brandId = input?.brand_id;
  useEffect(() => {
    if (!brandId) {
      setProducts([]);
      return;
    }
    listProducts(brandId, false)
      .then(setProducts)
      .catch(() => setProducts([]));
  }, [brandId]);

  // Which draft: a new one, or the stored one from the route.
  // biome-ignore lint/correctness/useExhaustiveDependencies: only the route's id decides
  useEffect(() => {
    const store = useDraft.getState();
    const first = !opened.current;
    opened.current = true;
    if (id && store.id === id) {
      // Just created here: stay on the step. Opened again: start where something is missing.
      if (first && store.input) {
        setFresh(false);
        setStep(firstOpenStep(store.input));
      }
      return;
    }
    if (!id) {
      if (store.id || store.input) store.open(null, null);
      setFresh(true);
      setStep(0);
      setDatesSeen(false);
      return;
    }
    let live = true;
    setFresh(false);
    loadInvoice(id)
      .then((invoice) => {
        if (!live) return;
        if (invoice.state !== "draft") {
          navigate({ page: "issued", id });
          return;
        }
        const loaded = toInput(invoice);
        useDraft.getState().open(id, loaded);
        setStep(firstOpenStep(loaded));
      })
      .catch((e) => toast("error", messageFor(toCmdError(e), t)));
    return () => {
      live = false;
    };
  }, [id]);

  // Once created, the route names the draft, so the menu and Ctrl+N know it. The store is
  // read here, not the render's values: "New invoice" has just cleared it in the effect above.
  // biome-ignore lint/correctness/useExhaustiveDependencies: draftId signals a new draft
  useEffect(() => {
    const created = useDraft.getState().id;
    if (created && !id) navigate({ page: "assistant", id: created });
  }, [draftId, id, navigate]);

  // Leaving the page writes what is still waiting.
  useEffect(
    () => () => {
      void flushDraft();
    },
    [],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.ctrlKey && !e.altKey && !e.shiftKey && e.key.toLowerCase() === "s") {
        e.preventDefault();
        void flushDraft();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  // A new step moves the focus to its question, for keyboard and screen readers.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs on every change of step
  useEffect(() => {
    if (!mounted.current) {
      mounted.current = true;
      return;
    }
    const cell = focusCell.current;
    focusCell.current = null;
    const target = cell
      ? document.querySelector<HTMLElement>(`[data-row="${cell.row}"][data-col="${cell.col}"]`)
      : null;
    (target ?? document.getElementById("question"))?.focus();
  }, [step]);

  const goTo = (index: number) => {
    setStep(index);
    if (index === 3) setDatesSeen(true);
  };

  /** "Go to field" from the check: the right step, and the line cell if there is one. */
  const jump = (target: IssueTarget) => {
    if ("page" in target) {
      navigate({ page: target.page });
      return;
    }
    if (target.row !== undefined && target.col)
      focusCell.current = { row: target.row, col: target.col };
    goTo(target.step);
  };

  const brand = brands.find((b) => b.id === input?.brand_id);
  const business = businesses.find((b) => b.id === brand?.business_id);
  const businessId = business?.id;
  useEffect(() => {
    if (businessId) useUi.getState().setSender(businessId);
  }, [businessId]);
  const customer = customers.find((c) => c.id === input?.customer_id);
  const currency = business?.currency ?? "EUR";
  const lineCount = input?.lines.length ?? 0;

  const steps = [
    { title: t("Sender"), sub: brand?.name ?? t("Who issues it?") },
    { title: t("Customer"), sub: customer?.name ?? t("Who gets it?") },
    {
      title: t("Items"),
      sub: lineCount ? t("{n} line(s)", { n: String(lineCount) }) : t("What is charged?"),
    },
    { title: t("Dates"), sub: t("Date, service, payment term") },
    { title: t("Issue"), sub: t("Check, number is given") },
  ];
  const done = [!!input, !!input?.customer_id, lineCount > 0, datesSeen, false];
  const locked = [false, !input, !input, !input, !input];

  const status =
    save === "error"
      ? t("Not saved: {reason}", { reason: saveError ? messageFor(saveError, t) : "" })
      : save === "pending" || save === "saving"
        ? t("Saving…")
        : save === "saved"
          ? t("Draft, saved automatically")
          : t("Nothing saved yet");

  const discard = async () => {
    setConfirm(false);
    try {
      if (draftId) await deleteDraft(draftId);
      useDraft.getState().open(null, null);
      navigate({ page: "invoices" });
    } catch (e) {
      toast("error", messageFor(toCmdError(e), t));
    }
  };

  const retry = () => {
    const current = useDraft.getState().input;
    if (!current) return;
    useDraft.getState().change(current);
    void flushDraft();
  };

  return (
    <div className="assistant">
      <div className="assistant-main">
        <header className="assistant-top">
          <h1>{fresh ? t("New invoice") : t("Edit draft")}</h1>
          <span className={save === "error" ? "saved error" : "saved"} aria-live="polite">
            {status}
          </span>
          {save === "error" && (
            <button type="button" className="btn ghost" onClick={retry}>
              <RotateCcw size={16} aria-hidden="true" />
              {t("Try again")}
            </button>
          )}
          <button
            type="button"
            className="btn ghost"
            onClick={() => (draftId ? setConfirm(true) : navigate({ page: "invoices" }))}
          >
            <Trash2 size={16} aria-hidden="true" />
            {t("Discard")}
          </button>
        </header>

        <div className="assistant-steps">
          <StepTiles steps={steps} current={step} done={done} disabled={locked} onSelect={goTo} />
        </div>

        <div className="assistant-ask">
          {step === 0 && (
            <StepSender brands={brands} businesses={businesses} onDone={() => goTo(1)} />
          )}
          {step === 1 && (
            <StepCustomer customers={customers} reload={reloadCustomers} onDone={() => goTo(2)} />
          )}
          {step === 2 && <StepLines products={products} business={business} currency={currency} />}
          {step === 3 && <StepDates />}
          {step === REVIEW && <StepIssue onJump={jump} />}
        </div>

        <footer className="assistant-foot">
          {totalsError ? (
            <span className="sum error" role="status">
              <CircleAlert size={16} aria-hidden="true" />
              {messageFor(totalsError, t)}
            </span>
          ) : (
            <span className="sum">
              {t("So far")}{" "}
              <b className={save === "pending" || save === "saving" ? "num stale" : "num"}>
                {totals ? formatCents(totals.gross, currency, i18n.language) : "–"}
              </b>
              {business?.small_business && ` · ${t("no VAT (§ 19)")}`}
            </span>
          )}
          <button
            type="button"
            className="btn"
            disabled={step === 0}
            onClick={() => goTo(step - 1)}
          >
            {t("Back")}
          </button>
          {step < REVIEW && (
            <button
              type="button"
              className="btn primary"
              disabled={!input}
              onClick={() => goTo(step + 1)}
            >
              {t("Next: {step}", { step: steps[step + 1]?.title ?? "" })}
            </button>
          )}
        </footer>
      </div>

      <SidePanel
        title={t("Preview")}
        open={previewOpen}
        onToggle={() => updateSettings({ preview_open: !currentSettings().preview_open })}
      >
        <Preview />
      </SidePanel>

      {confirm && (
        <ConfirmDialog
          title={t("Discard this draft?")}
          text={t("The draft is deleted. Nothing has been issued yet, so no number is lost.")}
          confirmLabel={t("Discard")}
          danger
          onCancel={() => setConfirm(false)}
          onConfirm={() => void discard()}
        />
      )}
    </div>
  );
}
