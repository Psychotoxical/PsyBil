import { useTranslation } from "react-i18next";
import { LANGUAGES, type Language } from "../../i18n";
import type { ThemeChoice } from "../../ipc/bindings/ThemeChoice";
import { PageHeader } from "../../shell/PageHeader";
import { useSettings } from "../../store/settings";
import { Choice } from "../../ui/Choice";

/** Appearance and language of this computer, and what PsyBil is. */
export function Settings() {
  const { t } = useTranslation();
  const theme = useSettings((s) => s.settings?.theme ?? "system");
  const language = useSettings((s) => s.settings?.language ?? "de");
  const update = useSettings((s) => s.update);

  return (
    <>
      <PageHeader title={t("Settings")} sub={t("These apply to this computer only.")} />
      <div className="settings">
        <section className="form-section">
          <h2 className="settings-title">{t("Look and language")}</h2>
          <Choice<ThemeChoice>
            label={t("Colour scheme")}
            value={theme}
            onChange={(v) => update({ theme: v })}
            options={[
              { value: "system", label: t("As the system") },
              { value: "dark", label: t("Dark") },
              { value: "light", label: t("Light") },
            ]}
          />
          <Choice<Language>
            label={t("Language")}
            value={language === "en" ? "en" : "de"}
            onChange={(v) => update({ language: v })}
            options={LANGUAGES.map(([value, label]) => ({ value, label }))}
          />
        </section>

        <section className="form-section about">
          <h2 className="settings-title">{t("About PsyBil")}</h2>
          <p>
            {t("Version")} <span className="num">{__APP_VERSION__}</span>
          </p>
          <p className="muted">
            {t(
              "PsyBil is free software under the GNU GPL 3.0. The fonts Inter, Space Grotesk and JetBrains Mono are under the SIL Open Font License 1.1; Inter is embedded in every PDF.",
            )}
          </p>
        </section>
      </div>
    </>
  );
}
