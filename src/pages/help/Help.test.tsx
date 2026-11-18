import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import { setLanguage } from "../../i18n";
import { useUi } from "../../store/ui";
import { ARTICLES } from "./articles";
import { Help } from "./Help";

describe("Help", () => {
  beforeEach(() => {
    setLanguage("de");
    useUi.setState({ route: { page: "help" } });
  });

  it("help_articles_match_in_both_languages", () => {
    const ids = (lang: "de" | "en") => ARTICLES[lang].map((a) => a.id);
    expect(ids("de")).toEqual(ids("en"));
    for (const id of ["first-invoice", "partial-payment", "cancel", "small-business"]) {
      expect(ids("de")).toContain(id);
    }
    for (const lang of ["de", "en"] as const) {
      for (const a of ARTICLES[lang]) {
        expect(a.title.trim()).not.toBe("");
        expect(a.intro.trim()).not.toBe("");
        expect(a.steps.length).toBeGreaterThan(1);
        for (const step of a.steps) expect(step.trim()).not.toBe("");
      }
    }
    // The same pages to jump to in both languages.
    expect(ARTICLES.de.map((a) => a.link)).toEqual(ARTICLES.en.map((a) => a.link));
  });

  it("help_link_navigates_to_page", async () => {
    const user = userEvent.setup();
    render(<Help />);
    await user.click(screen.getByRole("button", { name: "Kleinunternehmer" }));
    expect(screen.getByRole("heading", { level: 2 })).toHaveTextContent("Kleinunternehmer");
    expect(screen.getByText(/keine Steuerberatung/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Zur Übersicht" }));
    expect(useUi.getState().route).toEqual({ page: "overview" });
  });

  it("help_follows_the_language", async () => {
    setLanguage("en");
    render(<Help />);
    expect(screen.getByRole("button", { name: "First invoice" })).toBeInTheDocument();
    setLanguage("de");
  });
});
