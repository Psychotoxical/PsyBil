import i18next from "i18next";
import { initReactI18next } from "react-i18next";

/**
 * The English text is the key, as in PsyAudio. Every other language is one file in `locales/`:
 * `{"English": "translation"}`. A text a language lacks stays English. Placeholders are
 * `{name}`.
 */
type LanguageFile = Record<string, string>;

const files = import.meta.glob<LanguageFile>("../locales/*.json", {
  eager: true,
  import: "default",
});

export type Language = "de" | "en";

/** Code and name in its own language. */
export const LANGUAGES: [Language, string][] = [
  ["de", "Deutsch"],
  ["en", "English"],
];

export function languageFile(code: string): LanguageFile {
  return files[`../locales/${code}.json`] ?? {};
}

export const i18n = i18next.createInstance();

void i18n.use(initReactI18next).init({
  resources: Object.fromEntries(
    LANGUAGES.map(([code]) => [code, { translation: languageFile(code) }]),
  ),
  lng: "de",
  fallbackLng: false,
  keySeparator: false,
  nsSeparator: false,
  returnEmptyString: false,
  interpolation: { prefix: "{", suffix: "}", escapeValue: false },
  initAsync: false,
});

export function setLanguage(code: Language): void {
  void i18n.changeLanguage(code);
  document.documentElement.lang = code;
}
