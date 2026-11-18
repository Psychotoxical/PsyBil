import "./overview.css";
import { Plus } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Business } from "../../ipc/bindings/Business";
import type { Overview as OverviewData } from "../../ipc/bindings/Overview";
import type { SmallBusinessMeter as Meter } from "../../ipc/bindings/SmallBusinessMeter";
import { messageFor, toCmdError } from "../../ipc/errors";
import { listBusinesses } from "../../ipc/masterdata";
import { overview } from "../../ipc/overview";
import { PageHeader } from "../../shell/PageHeader";
import { useUi } from "../../store/ui";
import { useReadOnly } from "../../store/workspace";
import { DebtorList } from "./DebtorList";
import { OutsideRevenueDialog } from "./OutsideRevenueDialog";
import { ReceiptsChart } from "./ReceiptsChart";
import { SmallBusinessMeter } from "./SmallBusinessMeter";
import { Tiles } from "./Tiles";
import { YearSummary } from "./YearSummary";

/** The start page: who owes what, what came in, and how close the small-business limits are. */
export function Overview() {
  const { t } = useTranslation();
  const toast = useUi((s) => s.toast);
  const navigate = useUi((s) => s.navigate);
  const business = useUi((s) => s.overviewBusiness);
  const setBusiness = useUi((s) => s.setOverviewBusiness);
  const dataVersion = useUi((s) => s.dataVersion);
  const [businesses, setBusinesses] = useState<Business[]>([]);
  const [data, setData] = useState<OverviewData | null>(null);
  const [editing, setEditing] = useState<Meter | null>(null);
  const [failed, setFailed] = useState(false);
  const readOnly = useReadOnly();

  useEffect(() => {
    listBusinesses(false)
      .then(setBusinesses)
      .catch(() => setBusinesses([]));
  }, []);

  const load = useCallback(() => {
    let live = true;
    setFailed(false);
    overview(business)
      .then((found) => {
        if (live) setData(found);
      })
      .catch((error) => {
        if (!live) return;
        setFailed(true);
        toast("error", messageFor(toCmdError(error), t));
      });
    return () => {
      live = false;
    };
  }, [business, toast, t]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: dataVersion signals changed data
  useEffect(load, [load, dataVersion]);

  const choices = [{ id: null, name: t("All") }, ...businesses];
  const several = (data?.monthly.length ?? 0) > 1;

  return (
    <div className="overview">
      <PageHeader
        title={t("Overview")}
        actions={
          businesses.length > 1 && (
            <fieldset className="choice overview-filter">
              <legend className="visually-hidden">{t("Sender")}</legend>
              <div className="choice-row">
                {choices.map((b) => (
                  <button
                    key={b.id ?? "all"}
                    type="button"
                    className="choice-btn"
                    aria-pressed={business === b.id}
                    onClick={() => setBusiness(b.id)}
                  >
                    {b.name}
                  </button>
                ))}
              </div>
            </fieldset>
          )
        }
      />
      {failed && !data && (
        <div className="empty">
          <p>{t("The overview cannot be loaded.")}</p>
          <button type="button" className="btn" onClick={() => load()}>
            {t("Try once more")}
          </button>
        </div>
      )}
      {data && (
        <>
          <Tiles tiles={data.tiles} />
          <div className="overview-grid">
            <section className="overview-section" aria-labelledby="debtors-title">
              <h2 id="debtors-title">{t("Who owes what")}</h2>
              {data.debtors.length > 0 ? (
                <DebtorList debtors={data.debtors} />
              ) : (
                <div className="overview-empty">
                  <p>{t("Nobody owes you anything right now.")}</p>
                  <button
                    type="button"
                    className="btn"
                    disabled={readOnly !== null}
                    title={readOnly ?? undefined}
                    onClick={() => navigate({ page: "assistant" })}
                  >
                    <Plus size={16} aria-hidden="true" />
                    {t("New invoice")}
                  </button>
                </div>
              )}
            </section>
            <div className="overview-side">
              {data.meters.map((m) => (
                <SmallBusinessMeter
                  key={m.business_id}
                  meter={m}
                  named={data.meters.length > 1 || businesses.length > 1}
                  onEdit={() => setEditing(m)}
                />
              ))}
              {data.monthly.map((m) => (
                <ReceiptsChart key={m.currency} year={data.year} monthly={m} named={several} />
              ))}
            </div>
          </div>
          {businesses.length > 0 && <YearSummary businesses={businesses} business={business} />}
        </>
      )}
      {editing && (
        <OutsideRevenueDialog
          meter={editing}
          currency={businesses.find((b) => b.id === editing.business_id)?.currency ?? "EUR"}
          onClose={() => setEditing(null)}
          onSaved={() => {
            setEditing(null);
            load();
          }}
        />
      )}
    </div>
  );
}
