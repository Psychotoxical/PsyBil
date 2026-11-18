import "./help.css";
import { ArrowRight } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../../shell/PageHeader";
import { useUi } from "../../store/ui";
import { ARTICLES } from "./articles";

/** Short help sorted by task: topics on the left, the steps of one on the right. */
export function Help() {
  const { t, i18n } = useTranslation();
  const navigate = useUi((s) => s.navigate);
  const articles = ARTICLES[i18n.language.startsWith("en") ? "en" : "de"];
  const [id, setId] = useState(articles[0]?.id ?? "");
  const article = articles.find((a) => a.id === id) ?? articles[0];

  return (
    <div className="help">
      <PageHeader title={t("Help")} sub={t("Sorted by task. F1 opens it from anywhere.")} />
      <div className="help-body">
        <nav className="help-topics" aria-label={t("Topics")}>
          <ul>
            {articles.map((a) => (
              <li key={a.id}>
                <button
                  type="button"
                  className="help-topic"
                  aria-current={a.id === article?.id ? "true" : undefined}
                  onClick={() => setId(a.id)}
                >
                  {a.title}
                </button>
              </li>
            ))}
          </ul>
        </nav>
        {article && (
          <article className="help-article">
            <h2>{article.title}</h2>
            <p className="help-intro">{article.intro}</p>
            <ol className="help-steps">
              {article.steps.map((step) => (
                <li key={step}>{step}</li>
              ))}
            </ol>
            <button type="button" className="btn" onClick={() => navigate({ page: article.link })}>
              {article.linkLabel}
              <ArrowRight size={16} aria-hidden="true" />
            </button>
          </article>
        )}
      </div>
    </div>
  );
}
