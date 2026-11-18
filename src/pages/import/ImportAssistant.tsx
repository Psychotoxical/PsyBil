import "./import.css";
import { open } from "@tauri-apps/plugin-dialog";
import { CircleAlert, CircleCheck, Database, FolderOpen } from "lucide-react";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents, formatDate } from "../../format";
import type { ImportPlan } from "../../ipc/bindings/ImportPlan";
import type { ImportReport } from "../../ipc/bindings/ImportReport";
import type { LegacyClass } from "../../ipc/bindings/LegacyClass";
import type { ReviewReason } from "../../ipc/bindings/ReviewReason";
import { messageFor, toCmdError } from "../../ipc/errors";
import {
  applyImport,
  cancelImport,
  findLegacyDatabases,
  type LegacyPreview,
  readLegacy,
} from "../../ipc/legacy";
import { PageHeader } from "../../shell/PageHeader";
import { StepTiles } from "../../shell/StepTiles";
import { useUi } from "../../store/ui";

const CLASSES: LegacyClass[] = ["NeverSent", "SentOpen", "SentPaid", "Cancelled"];

function useTexts() {
  const { t } = useTranslation();
  const classes: Record<LegacyClass, string> = {
    NeverSent: t("never sent (draft)"),
    SentOpen: t("sent, open"),
    SentPaid: t("sent, paid"),
    Cancelled: t("cancelled"),
  };
  const reasons: Record<ReviewReason, string> = {
    TotalsMismatch: t("Net plus tax is not the gross amount"),
    ShippingTaxUnclear: t("Tax on shipping is unclear"),
    ZeroRateWithoutReason: t("0 % without a reason"),
    MissingCustomer: t("Customer missing"),
    DuplicateNumber: t("Number used twice"),
  };
  return { classes, reasons };
}

/** Step 1: which old database. */
function FileStep({ onRead, busy }: { onRead: (path: string) => void; busy: boolean }) {
  const { t } = useTranslation();
  const [found, setFound] = useState<string[] | null>(null);
  useEffect(() => {
    findLegacyDatabases()
      .then(setFound)
      .catch(() => setFound([]));
  }, []);
  const pick = async () => {
    const picked = await open({ filters: [{ name: "VibeBill", extensions: ["db"] }] });
    if (typeof picked === "string") onRead(picked);
  };
  return (
    <section className="import-step">
      <h2 className="import-question">{t("Which VibeBill data should PsyBil take over?")}</h2>
      <p className="field-hint">{t("PsyBil only reads a copy; the old file stays as it is.")}</p>
      <div className="answers">
        {found?.map((path) => (
          <button
            key={path}
            type="button"
            className="answer"
            disabled={busy}
            onClick={() => onRead(path)}
          >
            <span className="answer-icon">
              <Database size={22} aria-hidden="true" />
            </span>
            <span className="answer-title">{t("Found on this computer")}</span>
            <span className="answer-sub num">{path}</span>
          </button>
        ))}
        <button type="button" className="answer add" disabled={busy} onClick={() => void pick()}>
          <span className="answer-icon">
            <FolderOpen size={22} aria-hidden="true" />
          </span>
          <span className="answer-title">{t("Choose another file…")}</span>
          <span className="answer-sub">{t("For example a copy from another computer.")}</span>
        </button>
      </div>
      {found?.length === 0 && (
        <p className="field-hint">{t("No VibeBill data found in the usual place.")}</p>
      )}
    </section>
  );
}

type SendersProps = {
  plan: ImportPlan;
  answered: Record<number, boolean>;
  onAnswer: (index: number, small: boolean) => void;
  onMerge: (from: number, into: number) => void;
};

/** Step 2: one business per old sender; small business yes or no; senders that belong together. */
function SendersStep({ plan, answered, onAnswer, onMerge }: SendersProps) {
  const { t } = useTranslation();
  return (
    <section className="import-step">
      <h2 className="import-question">{t("Who were the senders?")}</h2>
      <p className="field-hint">
        {t(
          "Each old sender becomes a sender with one brand. Senders of the same business can become brands of one business.",
        )}
      </p>
      <ul className="import-senders">
        {plan.businesses.map((b, i) => (
          <li key={b.sellers.join("-")}>
            <div className="import-sender-name">
              <span className="answer-title">{b.name}</span>
              {b.existing && (
                <span className="field-hint">{t("adds to the sender taken over before")}</span>
              )}
              {b.sellers.length > 1 && (
                <span className="field-hint">
                  {t("{n} old senders as brands", { n: String(b.sellers.length) })}
                </span>
              )}
            </div>
            <fieldset className="choice" aria-label={t("Small business? {name}", { name: b.name })}>
              <legend className="field-label">{t("Small business (§ 19)?")}</legend>
              <div className="choice-row">
                {[true, false].map((value) => (
                  <button
                    key={String(value)}
                    type="button"
                    className="choice-btn"
                    aria-pressed={answered[i] === undefined ? false : b.small_business === value}
                    onClick={() => onAnswer(i, value)}
                  >
                    {value ? t("Yes") : t("No")}
                  </button>
                ))}
              </div>
            </fieldset>
            {plan.businesses.length > 1 && (
              <label className="field import-belongs">
                <span className="field-label">{t("Belongs to")}</span>
                <select
                  className="field-input"
                  value=""
                  onChange={(e) => {
                    if (e.target.value !== "") onMerge(i, Number(e.target.value));
                  }}
                >
                  <option value="">{t("its own business")}</option>
                  {plan.businesses.map((other, j) =>
                    j === i ? null : (
                      <option key={other.sellers.join("-")} value={j}>
                        {t("a brand of {name}", { name: other.name })}
                      </option>
                    ),
                  )}
                </select>
              </label>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

type MatchesProps = {
  preview: LegacyPreview;
  plan: ImportPlan;
  onToggle: (kind: "customer" | "product", legacyId: number, join: boolean) => void;
};

/** Step 3: customers and items PsyBil already has. */
function MatchesStep({ preview, plan, onToggle }: MatchesProps) {
  const { t } = useTranslation();
  const joined = (pairs: [number, string | null][], id: number) =>
    pairs.find(([legacy]) => legacy === id)?.[1] != null;
  const rows = [
    ...preview.customer_matches.map((m) => ({ kind: "customer" as const, m })),
    ...preview.product_matches.map((m) => ({ kind: "product" as const, m })),
  ];
  return (
    <section className="import-step">
      <h2 className="import-question">{t("Customers and items PsyBil knows already")}</h2>
      {rows.length === 0 ? (
        <p className="field-hint">{t("No customer or item appears twice.")}</p>
      ) : (
        <ul className="import-matches">
          {rows.map(({ kind, m }) => (
            <li key={`${kind}-${m.legacy_id}`}>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={joined(
                    kind === "customer" ? plan.customer_matches : plan.product_matches,
                    m.legacy_id,
                  )}
                  onChange={(e) => onToggle(kind, m.legacy_id, e.target.checked)}
                />
                <span className="switch-track" aria-hidden="true" />
                <span>
                  {kind === "customer"
                    ? t("Customer “{name}”: use the one in PsyBil", { name: m.legacy_name })
                    : t("Item “{name}”: use the one in PsyBil", { name: m.legacy_name })}
                </span>
              </label>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

type InvoicesProps = {
  preview: LegacyPreview;
  plan: ImportPlan;
  onClass: (legacyId: number, value: LegacyClass) => void;
};

/** Step 4: every old invoice with its proposed place; VibeBill's status is not reliable. */
function InvoicesStep({ preview, plan, onClass }: InvoicesProps) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const { classes, reasons } = useTexts();
  const [onlyReview, setOnlyReview] = useState(false);
  const rows = preview.invoices
    .map((row) => ({ row, planned: plan.invoices.find((p) => p.legacy_id === row.legacy_id) }))
    .filter(({ planned }) => planned && (!onlyReview || planned.review.length > 0));
  return (
    <section className="import-step">
      <h2 className="import-question">{t("What happened to each invoice?")}</h2>
      <p className="field-hint">
        {t(
          "VibeBill kept every invoice as a draft with a number. Please check the proposal in each row.",
        )}
      </p>
      <label className="switch">
        <input
          type="checkbox"
          checked={onlyReview}
          onChange={(e) => setOnlyReview(e.target.checked)}
        />
        <span className="switch-track" aria-hidden="true" />
        <span>{t("Show only those to check")}</span>
      </label>
      <table className="invoice-table import-table">
        <thead>
          <tr>
            <th scope="col">{t("Number")}</th>
            <th scope="col">{t("Date")}</th>
            <th scope="col">{t("Customer")}</th>
            <th scope="col" className="end">
              {t("Amount")}
            </th>
            <th scope="col">{t("What happened")}</th>
          </tr>
        </thead>
        <tbody>
          {rows.map(({ row, planned }) => (
            <tr key={row.legacy_id}>
              <td className="num">{row.number}</td>
              <td className="num">{formatDate(row.date.slice(0, 10), lang)}</td>
              <td>
                {row.customer}
                {planned && planned.review.length > 0 && (
                  <span className="import-review">
                    {planned.review.map((r) => (
                      <span key={r} className="badge check">
                        <CircleAlert size={14} aria-hidden="true" />
                        {reasons[r]}
                      </span>
                    ))}
                  </span>
                )}
              </td>
              <td className="num end">{formatCents(row.gross, "EUR", lang)}</td>
              <td>
                <select
                  className="field-input import-class"
                  aria-label={t("Placement {number}", { number: row.number })}
                  value={planned?.class}
                  onChange={(e) => onClass(row.legacy_id, e.target.value as LegacyClass)}
                >
                  {CLASSES.map((c) => (
                    <option key={c} value={c}>
                      {classes[c]}
                    </option>
                  ))}
                </select>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {rows.length === 0 && <p className="field-hint">{t("No invoice to check.")}</p>}
    </section>
  );
}

/** The result: counts and warnings; everything is in the log. */
/** "1 draft" or "3 drafts": the first key for one, the second with `{n}`. */
function useCount() {
  const { t } = useTranslation();
  return (n: number, one: string, many: string) => (n === 1 ? t(one) : t(many, { n: String(n) }));
}

function ReportView({ report, warnings }: { report: ImportReport; warnings: string[] }) {
  const { t } = useTranslation();
  const navigate = useUi((s) => s.navigate);
  const count = useCount();
  const lines = [
    count(report.businesses, "1 sender", "{n} senders"),
    count(report.customers, "1 customer", "{n} customers"),
    count(report.products, "1 item", "{n} items"),
    count(report.drafts, "1 draft", "{n} drafts"),
    count(report.issued, "1 invoice issued (taken over)", "{n} invoices issued (taken over)"),
    count(report.payments, "1 payment", "{n} payments"),
    count(report.needs_review, "1 marked for checking", "{n} marked for checking"),
  ];
  if (report.already_present > 0)
    lines.push(
      count(report.already_present, "1 was taken over before", "{n} were taken over before"),
    );
  if (report.quarantined > 0)
    lines.push(
      count(
        report.quarantined,
        "1 could not be placed and is kept aside",
        "{n} could not be placed and are kept aside",
      ),
    );
  return (
    <section className="import-step">
      <h2 className="import-question">
        <CircleCheck size={26} aria-hidden="true" className="import-done" />
        {t("Taken over")}
      </h2>
      <ul className="import-report">
        {lines.map((line) => (
          <li key={line}>{line}</li>
        ))}
      </ul>
      {warnings.length > 0 && (
        <>
          <h3>{t("Notes")}</h3>
          <ul className="import-report warn">
            {warnings.map((w) => (
              <li key={w}>
                <CircleAlert size={16} aria-hidden="true" />
                <span>{w}</span>
              </li>
            ))}
          </ul>
        </>
      )}
      <p className="field-hint">
        {t("Everything is recorded in the log. A backup was made before.")}
      </p>
      <button type="button" className="btn primary" onClick={() => navigate({ page: "invoices" })}>
        {t("To the invoices")}
      </button>
    </section>
  );
}

/** Takes over a VibeBill database: file, senders, matches, every invoice, then the report. */
export function ImportAssistant() {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const navigate = useUi((s) => s.navigate);
  const reasonId = useId();
  const [step, setStep] = useState(0);
  const [preview, setPreview] = useState<LegacyPreview | null>(null);
  const [plan, setPlan] = useState<ImportPlan | null>(null);
  const [answered, setAnswered] = useState<Record<number, boolean>>({});
  const [report, setReport] = useState<ImportReport | null>(null);
  const [busy, setBusy] = useState(false);

  const fail = (error: unknown) => toast("error", messageFor(toCmdError(error), t));

  const read = async (path: string) => {
    setBusy(true);
    try {
      const found = await readLegacy(path);
      setPreview(found);
      setPlan(structuredClone(found.plan));
      setAnswered({});
      setStep(1);
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };

  const cancel = async () => {
    await cancelImport().catch(() => {});
    navigate({ page: "settings" });
  };

  const take = async () => {
    if (!preview || !plan) return;
    setBusy(true);
    try {
      setReport(await applyImport(preview.token, plan));
      useUi.getState().bump();
    } catch (error) {
      fail(error);
    } finally {
      setBusy(false);
    }
  };

  const unanswered = plan ? plan.businesses.filter((_, i) => answered[i] === undefined) : [];
  const missing =
    step === 1 && unanswered.length > 0
      ? t("Small business? missing: {names}", { names: unanswered.map((b) => b.name).join(", ") })
      : null;

  const steps = [
    { title: t("File") },
    { title: t("Senders") },
    { title: t("Customers and items") },
    { title: t("Invoices") },
    { title: t("Summary") },
  ];
  const done = steps.map((_, i) => i < step && plan !== null);
  const disabled = steps.map(
    (_, i) => i > 0 && (plan === null || (i > 1 && unanswered.length > 0)),
  );

  const counts = plan
    ? CLASSES.map((c) => ({ c, n: plan.invoices.filter((i) => i.class === c).length }))
    : [];
  const { classes } = useTexts();
  const count = useCount();

  if (report && preview) {
    return (
      <div className="import">
        <PageHeader title={t("Take over from VibeBill")} />
        <ReportView report={report} warnings={preview.warnings} />
      </div>
    );
  }

  return (
    <div className="import">
      <PageHeader title={t("Take over from VibeBill")} />
      <StepTiles steps={steps} current={step} done={done} disabled={disabled} onSelect={setStep} />
      {step === 0 && <FileStep busy={busy} onRead={(path) => void read(path)} />}
      {step === 1 && plan && (
        <SendersStep
          plan={plan}
          answered={answered}
          onAnswer={(i, small) => {
            setPlan({
              ...plan,
              businesses: plan.businesses.map((b, j) =>
                j === i ? { ...b, small_business: small } : b,
              ),
            });
            setAnswered({ ...answered, [i]: true });
          }}
          onMerge={(from, into) => {
            const target = plan.businesses[into];
            const moved = plan.businesses[from];
            if (!target || !moved) return;
            const businesses = plan.businesses
              .map((b, j) =>
                j === into ? { ...target, sellers: [...target.sellers, ...moved.sellers] } : b,
              )
              .filter((_, j) => j !== from);
            // Answers follow their business to its new place in the list.
            const next: Record<number, boolean> = {};
            plan.businesses.forEach((_, j) => {
              if (j === from || answered[j] === undefined) return;
              next[j > from ? j - 1 : j] = true;
            });
            setPlan({ ...plan, businesses });
            setAnswered(next);
          }}
        />
      )}
      {step === 2 && plan && preview && (
        <MatchesStep
          preview={preview}
          plan={plan}
          onToggle={(kind, legacyId, join) => {
            const list = kind === "customer" ? preview.customer_matches : preview.product_matches;
            const existing = list.find((m) => m.legacy_id === legacyId)?.existing_id ?? null;
            const key = kind === "customer" ? "customer_matches" : "product_matches";
            setPlan({
              ...plan,
              [key]: plan[key].map(([id, current]) =>
                id === legacyId ? [id, join ? existing : null] : [id, current],
              ),
            });
          }}
        />
      )}
      {step === 3 && plan && preview && (
        <InvoicesStep
          preview={preview}
          plan={plan}
          onClass={(legacyId, value) =>
            setPlan({
              ...plan,
              invoices: plan.invoices.map((i) =>
                i.legacy_id === legacyId ? { ...i, class: value } : i,
              ),
            })
          }
        />
      )}
      {step === 4 && plan && (
        <section className="import-step">
          <h2 className="import-question">{t("Ready to take over")}</h2>
          <ul className="import-report">
            <li>{count(plan.businesses.length, "1 sender", "{n} senders")}</li>
            {counts.map(({ c, n }) => (
              <li key={c}>
                {n === 1
                  ? t("1 invoice: {placement}", { placement: classes[c] })
                  : t("{n} invoices: {placement}", { n: String(n), placement: classes[c] })}
              </li>
            ))}
          </ul>
          <p className="field-hint">{t("PsyBil makes a backup first.")}</p>
        </section>
      )}
      {step > 0 && (
        <div className="form-bar import-bar">
          <div className="form-bar-extra">
            <button type="button" className="btn ghost" onClick={() => void cancel()}>
              {t("Cancel")}
            </button>
          </div>
          {missing && (
            <p className="form-missing" id={reasonId}>
              <span>{missing}</span>
            </p>
          )}
          <button type="button" className="btn" onClick={() => setStep(step - 1)}>
            {t("Back")}
          </button>
          {step < 4 ? (
            <button
              type="button"
              className="btn primary"
              disabled={missing !== null || busy}
              aria-describedby={missing ? reasonId : undefined}
              onClick={() => setStep(step + 1)}
            >
              {t("Next")}
            </button>
          ) : (
            <button
              type="button"
              className="btn primary"
              disabled={busy}
              onClick={() => void take()}
            >
              {t("Take over")}
            </button>
          )}
        </div>
      )}
    </div>
  );
}
