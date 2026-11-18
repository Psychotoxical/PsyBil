import type { Page } from "../../store/ui";

/** One help article: what it is about and the steps, in one language. */
export type Article = {
  id: string;
  title: string;
  intro: string;
  steps: string[];
  /** The page the article is about, offered as a button. */
  link: Page;
  linkLabel: string;
};

/**
 * The help, sorted by task. The texts are written per language here instead
 * of going through `t()`, because whole paragraphs make poor keys.
 */
export const ARTICLES: Record<"de" | "en", Article[]> = {
  de: [
    {
      id: "first-invoice",
      title: "Erste Rechnung",
      intro: "Einmal einrichten, dann führt der Assistent Schritt für Schritt durch jede Rechnung.",
      steps: [
        "Unter „Absender“ dein Unternehmen mit Anschrift, Steuernummer und Bankverbindung anlegen; Logo und Farbe gehören zur Marke.",
        "„Neue Rechnung“ (Strg+N) öffnen und den Absender wählen.",
        "Den Kunden suchen oder direkt neu anlegen.",
        "Positionen eintragen: aus dem Katalog oder frei, dazu Versand oder Rabatt. Rechts wächst das echte PDF mit.",
        "Datum und Leistungsdatum prüfen, dann „Ausstellen …“. Erst jetzt bekommt die Rechnung ihre Nummer und wird eingefroren.",
        "„PDF speichern“ und die Datei an den Kunden schicken.",
      ],
      link: "senders",
      linkLabel: "Zu den Absendern",
    },
    {
      id: "partial-payment",
      title: "Teilzahlung",
      intro: "Eine Rechnung darf in mehreren Teilen bezahlt werden; PsyBil rechnet den Rest aus.",
      steps: [
        "Die Rechnung in der Liste öffnen.",
        "„Zahlung erfassen“ wählen, Betrag, Datum und Art eintragen.",
        "Der Status zeigt „teilbezahlt“ und den offenen Rest; die Übersicht zählt ihn bei „Wer schuldet mir was“.",
        "Kommt der Rest, wieder „Zahlung erfassen“. Ist alles da, steht die Rechnung auf „bezahlt“.",
        "Eine falsch erfasste Zahlung lässt sich in der Rechnung wieder löschen.",
      ],
      link: "invoices",
      linkLabel: "Zu den Rechnungen",
    },
    {
      id: "cancel",
      title: "Storno und Korrigieren",
      intro:
        "Eine ausgestellte Rechnung ändert sich nie wieder. Fehler behebt ein Storno, auf Wunsch gleich mit einer neuen Rechnung.",
      steps: [
        "Die Rechnung öffnen.",
        "„Stornieren“ legt einen Storno-Beleg mit eigener Nummer an, der den Betrag aufhebt. Beide bleiben im Archiv.",
        "„Korrigieren“ storniert ebenso und legt sofort einen Entwurf mit demselben Inhalt an; Zahlungen wandern mit.",
        "Den Entwurf richtigstellen und neu ausstellen. Die neue Rechnung nennt, was schon bezahlt ist.",
      ],
      link: "invoices",
      linkLabel: "Zu den Rechnungen",
    },
    {
      id: "small-business",
      title: "Kleinunternehmer",
      intro:
        "Mit § 19 UStG weist eine Rechnung keine Umsatzsteuer aus. PsyBil setzt den Hinweis und zeigt, wie nah die Grenzen sind.",
      steps: [
        "Beim Absender „Kleinunternehmer“ einschalten. Jede Rechnung trägt dann den Hinweis auf § 19 und rechnet ohne Steuer.",
        "Die Übersicht zeigt zwei Grenzen: Im Vorjahr höchstens 25.000 €, im laufenden Jahr höchstens 100.000 €. Ab 80 % gibt es eine Warnung.",
        "Gezählt wird Geld, das angekommen ist. Umsätze außerhalb von PsyBil (Plattformen, Bargeld am Merchstand) trägst du dort je Jahr von Hand ein, auch fürs Vorjahr.",
        "Liegt dieses Jahr über 25.000 €, gilt die Regelung im nächsten Jahr nicht mehr; über 100.000 € endet sie schon im selben Jahr.",
        "Das ist ein Hinweis, keine Steuerberatung. Im Zweifel die Steuerberatung fragen.",
      ],
      link: "overview",
      linkLabel: "Zur Übersicht",
    },
    {
      id: "several-computers",
      title: "Mehrere Rechner",
      intro:
        "Mit einem gemeinsamen Ordner, etwa in der Nextcloud, haben mehrere Rechner dieselben Daten.",
      steps: [
        "In den Einstellungen unter „Datenordner“ einen gemeinsamen Ordner wählen. PsyBil zieht die Daten dorthin um.",
        "Auf den anderen Rechnern denselben Ordner wählen und „Dort öffnen“ nehmen.",
        "Es schreibt immer nur ein Rechner. Die anderen sehen oben „Wird gerade auf … bearbeitet“ und können lesen.",
        "Ist ein Rechner abgestürzt, lässt sich seine Sperre übernehmen. „Trotzdem übernehmen“ ist nur für den Notfall.",
        "Legt die Cloud eine Konfliktkopie an, bietet PsyBil beim Start das Zusammenführen an. Sicherungen bleiben auf jedem Rechner für sich.",
      ],
      link: "settings",
      linkLabel: "Zu den Einstellungen",
    },
    {
      id: "backup",
      title: "Sicherung",
      intro: "PsyBil sichert jeden Tag von selbst. Von Hand geht es jederzeit.",
      steps: [
        "Beim ersten Start des Tages entsteht eine Sicherung; die letzten 10 bleiben liegen.",
        "In den Einstellungen „Jetzt sichern …“ wählen und einen Ort aussuchen, etwa einen USB-Stick.",
        "Zum Wiederherstellen eine Sicherung aus der Liste oder eine Datei wählen. PsyBil zeigt Datum und Zahl der Belege und fragt nach.",
        "Vor dem Tausch sichert PsyBil den jetzigen Stand, so geht nichts verloren.",
      ],
      link: "settings",
      linkLabel: "Zu den Einstellungen",
    },
    {
      id: "import",
      title: "Aus VibeBill übernehmen",
      intro:
        "Der Assistent holt Absender, Kunden, Artikel und Rechnungen aus VibeBill. Die alte Datei wird nur gelesen.",
      steps: [
        "In den Einstellungen „Aus VibeBill übernehmen …“ öffnen und die gefundene Datei oder eine andere wählen.",
        "Für jeden alten Verkäufer angeben, ob er Kleinunternehmer ist; zusammengehörige lassen sich als Marken zusammenlegen.",
        "Doppelte Kunden und Artikel zusammenlegen oder getrennt lassen.",
        "Jede alte Rechnung einordnen: nie verschickt, verschickt und offen, verschickt und bezahlt oder storniert. VibeBill hat das nicht zuverlässig gespeichert.",
        "„Übernehmen“ legt vorher eine Sicherung an. Danach lässt sich an jede übernommene Rechnung das damals verschickte PDF anhängen.",
      ],
      link: "import",
      linkLabel: "Zur Übernahme",
    },
  ],
  en: [
    {
      id: "first-invoice",
      title: "First invoice",
      intro: "Set up once; then the assistant takes you through every invoice step by step.",
      steps: [
        "Under “Senders”, add your business with address, tax number and bank details; logo and colour belong to the brand.",
        "Open “New invoice” (Ctrl+N) and choose the sender.",
        "Find the customer or add a new one right there.",
        "Add the lines: from the catalogue or free, plus shipping or a discount. The real PDF grows on the right.",
        "Check the date and the date of service, then “Issue…”. Only now the invoice gets its number and is frozen.",
        "“Save PDF” and send the file to the customer.",
      ],
      link: "senders",
      linkLabel: "To the senders",
    },
    {
      id: "partial-payment",
      title: "Partial payment",
      intro: "An invoice may be paid in several parts; PsyBil works out the rest.",
      steps: [
        "Open the invoice in the list.",
        "Choose “Record payment” and enter amount, date and method.",
        "The status shows “partly paid” and what is still open; the overview counts it under “Who owes what”.",
        "When the rest arrives, record it the same way. Once everything is there, the invoice says “paid”.",
        "A payment recorded by mistake can be deleted in the invoice.",
      ],
      link: "invoices",
      linkLabel: "To the invoices",
    },
    {
      id: "cancel",
      title: "Cancel and correct",
      intro:
        "An issued invoice never changes again. A cancellation fixes mistakes, if you like with a new invoice at once.",
      steps: [
        "Open the invoice.",
        "“Cancel” creates a cancellation with its own number that cancels the amount. Both stay in the archive.",
        "“Correct” cancels too and creates a draft with the same content right away; payments move along.",
        "Put the draft right and issue it again. The new invoice names what has already been paid.",
      ],
      link: "invoices",
      linkLabel: "To the invoices",
    },
    {
      id: "small-business",
      title: "Small business",
      intro:
        "With § 19 UStG an invoice shows no VAT. PsyBil adds the note and shows how close the limits are.",
      steps: [
        "Switch on “Small business” for the sender. Every invoice then carries the § 19 note and is calculated without tax.",
        "The overview shows two limits: at most €25,000 last year and at most €100,000 this year. From 80 % there is a warning.",
        "Money received is what counts. Revenue outside PsyBil (platforms, cash at the merch table) is entered there per year by hand, also for last year.",
        "Above €25,000 this year, the rule does not apply next year; above €100,000 it ends within the same year.",
        "This is a hint, not tax advice. When in doubt, ask a tax adviser.",
      ],
      link: "overview",
      linkLabel: "To the overview",
    },
    {
      id: "several-computers",
      title: "Several computers",
      intro:
        "With a shared folder, for example in a Nextcloud, several computers have the same data.",
      steps: [
        "In the settings under “Data folder”, choose a shared folder. PsyBil moves the data there.",
        "On the other computers choose the same folder and take “Open there”.",
        "Only one computer writes at a time. The others see “Being edited on …” at the top and can read.",
        "If a computer crashed, its lock can be taken over. “Take over anyway” is for emergencies only.",
        "If the cloud makes a conflict copy, PsyBil offers to merge it at start. Backups stay on each computer.",
      ],
      link: "settings",
      linkLabel: "To the settings",
    },
    {
      id: "backup",
      title: "Backup",
      intro: "PsyBil backs up by itself every day. By hand it works any time.",
      steps: [
        "The first start of a day makes a backup; the last 10 are kept.",
        "In the settings choose “Back up now…” and pick a place, for example a USB stick.",
        "To restore, choose a backup from the list or a file. PsyBil shows its date and the number of documents and asks first.",
        "Before the swap PsyBil backs up the current state, so nothing is lost.",
      ],
      link: "settings",
      linkLabel: "To the settings",
    },
    {
      id: "import",
      title: "Take over from VibeBill",
      intro:
        "The assistant brings senders, customers, items and invoices over from VibeBill. The old file is only read.",
      steps: [
        "In the settings open “Take over from VibeBill…” and choose the file found or another one.",
        "For each old sender say whether it is a small business; senders that belong together can become brands of one business.",
        "Merge customers and items that appear twice, or keep them apart.",
        "Place every old invoice: never sent, sent and open, sent and paid, or cancelled. VibeBill did not record this reliably.",
        "“Take over” makes a backup first. Afterwards the PDF sent at the time can be attached to each invoice taken over.",
      ],
      link: "import",
      linkLabel: "To the import",
    },
  ],
};
