import "./issued.css";
import { save } from "@tauri-apps/plugin-dialog";
import { Ban, Copy, FileDown, List, Pencil, Plus, Trash2, Wallet } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { formatCents, formatDate } from "../../format";
import type { Invoice } from "../../ipc/bindings/Invoice";
import type { InvoiceStatus } from "../../ipc/bindings/InvoiceStatus";
import type { Payment } from "../../ipc/bindings/Payment";
import type { PaymentMethod } from "../../ipc/bindings/PaymentMethod";
import { messageFor, toCmdError } from "../../ipc/errors";
import {
  cancelInvoice,
  correctInvoice,
  duplicateAsDraft,
  invoiceStatus,
  loadInvoice,
} from "../../ipc/invoices";
import { listCustomers } from "../../ipc/masterdata";
import { deletePayment, listPayments } from "../../ipc/payments";
import { exportPdf, pdfFileName } from "../../ipc/pdf";
import { ConfirmDialog } from "../../shell/ConfirmDialog";
import { useUi } from "../../store/ui";
import { StatusBadge } from "../../ui/StatusBadge";
import { PdfPages } from "../assistant/Preview";
import { PaymentDialog } from "./PaymentDialog";

type Pending = { kind: "cancel" } | { kind: "correct" } | { kind: "payment"; payment: Payment };
type TrailKey = "cancelledBy" | "cancels" | "replaces";

/** A translated sentence with `{number}` set in the mono font. */
function WithNumber({ text, number }: { text: string; number: string }) {
  const [before, after] = text.split("\u0000");
  return (
    <>
      {before}
      <span className="num">{number}</span>
      {after}
    </>
  );
}

/** An issued invoice or cancellation: read only, with its archived PDF, status and payments. */
export function IssuedView({ id, fresh }: { id: string; fresh?: boolean }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language;
  const toast = useUi((s) => s.toast);
  const navigate = useUi((s) => s.navigate);
  const bump = useUi((s) => s.bump);
  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [status, setStatus] = useState<InvoiceStatus | null>(null);
  const [payments, setPayments] = useState<Payment[]>([]);
  const [customer, setCustomer] = useState<string | null>(null);
  const [paying, setPaying] = useState(false);
  const [pending, setPending] = useState<Pending | null>(null);
  const [version, setVersion] = useState(0);
  const [trail, setTrail] = useState<Partial<Record<TrailKey, { id: string; number: string }>>>({});

  const fail = useCallback(
    (e: unknown) => toast("error", messageFor(toCmdError(e), t)),
    [toast, t],
  );

  const reload = useCallback(async () => {
    try {
      const [found, state, paid] = await Promise.all([
        loadInvoice(id),
        invoiceStatus(id),
        listPayments(id),
      ]);
      if (found.state === "draft") {
        navigate({ page: "assistant", id });
        return;
      }
      setInvoice(found);
      setStatus(state);
      setPayments(paid);
      useUi.getState().setSender(found.business_id);
      // The numbers of the documents this one is linked with.
      const links = [
        ["cancelledBy", found.cancelled_by_id],
        ["cancels", found.cancels_id],
        ["replaces", found.replaces_id],
      ] as const;
      const numbers = await Promise.all(
        links.map(async ([key, other]) =>
          other
            ? ([key, { id: other, number: (await loadInvoice(other)).number ?? "" }] as const)
            : null,
        ),
      );
      setTrail(Object.fromEntries(numbers.filter((n) => n !== null)));
    } catch (e) {
      fail(e);
    }
  }, [id, navigate, fail]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const customerId = invoice?.customer_id;
  useEffect(() => {
    if (!customerId) return;
    listCustomers(true)
      .then((all) => setCustomer(all.find((c) => c.id === customerId)?.name ?? null))
      .catch(() => {});
  }, [customerId]);

  const savePdf = async () => {
    try {
      const name = await pdfFileName(id);
      const path = await save({
        defaultPath: name,
        filters: [{ name: "PDF", extensions: ["pdf"] }],
      });
      if (!path) return;
      const written = await exportPdf(id, path);
      toast("info", t("PDF saved: {path}", { path: written }));
    } catch (e) {
      fail(e);
    }
  };

  const cancel = async () => {
    setPending(null);
    try {
      await cancelInvoice(id);
      toast("info", t("Cancelled. The cancellation has its own number and is in the list."));
      bump();
      setVersion((v) => v + 1);
      await reload();
    } catch (e) {
      fail(e);
    }
  };

  const correct = async () => {
    setPending(null);
    try {
      const [, draft] = await correctInvoice(id);
      bump();
      navigate({ page: "assistant", id: draft });
    } catch (e) {
      fail(e);
    }
  };

  const duplicate = async () => {
    try {
      navigate({ page: "assistant", id: await duplicateAsDraft(id) });
    } catch (e) {
      fail(e);
    }
  };

  const removePayment = async (payment: Payment) => {
    setPending(null);
    try {
      await deletePayment(payment.id);
      await reload();
    } catch (e) {
      fail(e);
    }
  };

  if (!invoice) return null;

  const currency = invoice.currency;
  const money = (cents: number) => formatCents(cents, currency, lang);
  const storno = invoice.kind === "storno";
  const closed = storno || invoice.cancelled_by_id !== null || invoice.legacy_cancelled;
  const number = invoice.number ?? "";
  const methods: Record<PaymentMethod, string> = {
    transfer: t("Bank transfer"),
    cash: t("Cash"),
    pay_pal: "PayPal",
    other: t("Other"),
    legacy: t("From VibeBill"),
  };

  const marker = { number: "\u0000" };
  const title = fresh
    ? t("Issued: {number} is done.", marker)
    : storno
      ? t("Cancellation {number}", marker)
      : t("Invoice {number}", marker);
  const links: { key: TrailKey; text: string }[] = [
    { key: "cancelledBy", text: t("Cancelled by {number}", marker) },
    { key: "cancels", text: t("Cancels {number}", marker) },
    { key: "replaces", text: t("Replaces {number}", marker) },
  ];

  return (
    <div className="issued">
      <header className="page-header issued-head">
        <div>
          <h1>
            <WithNumber text={title} number={number} />
          </h1>
          <p className="page-sub issued-sub">
            {status && !storno && <StatusBadge status={status} />}
            <span>{customer ?? t("No customer yet")}</span>
            <span className="num">{formatDate(invoice.invoice_date, lang)}</span>
            {links.map(({ key, text }) => {
              const link = trail[key];
              if (!link) return null;
              return (
                <button
                  key={key}
                  type="button"
                  className="link-btn"
                  onClick={() => navigate({ page: "issued", id: link.id })}
                >
                  <WithNumber text={text} number={link.number} />
                </button>
              );
            })}
          </p>
        </div>
      </header>

      <div className="issued-actions">
        <button type="button" className="btn primary" onClick={() => void savePdf()}>
          <FileDown size={16} aria-hidden="true" />
          {t("Save PDF")}
        </button>
        {!closed && (
          <button type="button" className="btn" onClick={() => setPaying(true)}>
            <Wallet size={16} aria-hidden="true" />
            {t("Record payment")}
          </button>
        )}
        {fresh ? (
          <>
            <button type="button" className="btn" onClick={() => navigate({ page: "invoices" })}>
              <List size={16} aria-hidden="true" />
              {t("To the list")}
            </button>
            <button type="button" className="btn" onClick={() => navigate({ page: "assistant" })}>
              <Plus size={16} aria-hidden="true" />
              {t("New invoice")}
            </button>
          </>
        ) : (
          <>
            {!storno && (
              <button type="button" className="btn" onClick={() => void duplicate()}>
                <Copy size={16} aria-hidden="true" />
                {t("Duplicate as draft")}
              </button>
            )}
            {!closed && (
              <>
                <span className="issued-actions-gap" />
                <button
                  type="button"
                  className="btn ghost"
                  onClick={() => setPending({ kind: "correct" })}
                >
                  <Pencil size={16} aria-hidden="true" />
                  {t("Correct")}
                </button>
                <button
                  type="button"
                  className="btn ghost danger-text"
                  onClick={() => setPending({ kind: "cancel" })}
                >
                  <Ban size={16} aria-hidden="true" />
                  {t("Cancel invoice")}
                </button>
              </>
            )}
          </>
        )}
      </div>

      <div className="issued-body">
        <div className="issued-facts">
          <dl className="facts">
            <div>
              <dt>{t("Amount")}</dt>
              <dd className="num">{invoice.gross !== null ? money(invoice.gross) : "–"}</dd>
            </div>
            {status && !storno && (
              <div>
                <dt>{status.open < 0 ? t("Overpaid by") : t("Still open")}</dt>
                <dd className="num">{money(Math.abs(status.open))}</dd>
              </div>
            )}
            {!storno && (
              <div>
                <dt>{t("Due on")}</dt>
                <dd className="num">{formatDate(invoice.due_date, lang)}</dd>
              </div>
            )}
          </dl>

          {!storno && (
            <section className="payments">
              <h2>{t("Payments")}</h2>
              {payments.length === 0 ? (
                <p className="muted">{t("No payment recorded yet.")}</p>
              ) : (
                <table className="payment-table" aria-label={t("Payments")}>
                  <thead>
                    <tr>
                      <th scope="col">{t("Date")}</th>
                      <th scope="col">{t("Method")}</th>
                      <th scope="col" className="end">
                        {t("Amount")}
                      </th>
                      <th scope="col">{t("Note")}</th>
                      <th scope="col">
                        <span className="visually-hidden">{t("Actions")}</span>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    {payments.map((p) => (
                      <tr key={p.id}>
                        <td className="num">{formatDate(p.date, lang)}</td>
                        <td>{methods[p.method]}</td>
                        <td className="num end">{money(p.amount)}</td>
                        <td>
                          {p.note}
                          {p.transferred_to_id && <span className="tag">{t("transferred")}</span>}
                          {p.transfer_of_id && <span className="tag">{t("taken over")}</span>}
                        </td>
                        <td className="end">
                          {!p.transferred_to_id && (
                            <button
                              type="button"
                              className="icon-btn"
                              aria-label={t("Delete payment of {date}", {
                                date: formatDate(p.date, lang),
                              })}
                              onClick={() => setPending({ kind: "payment", payment: p })}
                            >
                              <Trash2 size={15} aria-hidden="true" />
                            </button>
                          )}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
            </section>
          )}
        </div>

        <div className="issued-pages">
          <PdfPages id={id} version={version} delay={0} />
        </div>
      </div>

      {paying && status && (
        <PaymentDialog
          invoiceId={id}
          open={status.open}
          currency={currency}
          onClose={() => setPaying(false)}
          onDone={() => {
            setPaying(false);
            toast("info", t("Payment recorded."));
            bump();
            void reload();
          }}
        />
      )}
      {pending?.kind === "cancel" && (
        <ConfirmDialog
          title={t("Cancel this invoice?")}
          text={t(
            "PsyBil creates a cancellation with its own number that cancels the amount. Both stay in the archive; the invoice can no longer be paid.",
          )}
          confirmLabel={t("Cancel invoice")}
          cancelLabel={t("Keep invoice")}
          danger
          onCancel={() => setPending(null)}
          onConfirm={() => void cancel()}
        />
      )}
      {pending?.kind === "correct" && (
        <ConfirmDialog
          title={t("Correct this invoice?")}
          text={t(
            "PsyBil cancels this invoice and opens a new draft with the same details. What was paid moves to the new invoice.",
          )}
          confirmLabel={t("Correct")}
          onCancel={() => setPending(null)}
          onConfirm={() => void correct()}
        />
      )}
      {pending?.kind === "payment" && (
        <ConfirmDialog
          title={t("Delete this payment?")}
          text={t("The payment of {amount} is removed and the open amount goes up again.", {
            amount: money(pending.payment.amount),
          })}
          confirmLabel={t("Delete")}
          danger
          onCancel={() => setPending(null)}
          onConfirm={() => void removePayment(pending.payment)}
        />
      )}
    </div>
  );
}
