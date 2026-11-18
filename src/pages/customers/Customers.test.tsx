import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { Customer } from "../../ipc/bindings/Customer";
import { deleteCustomer, updateCustomer } from "../../ipc/masterdata";
import { Toasts } from "../../shell/Toasts";
import { useUi } from "../../store/ui";
import { Customers } from "./Customers";

vi.mock("../../ipc/masterdata", () => ({
  listCustomers: vi.fn(async () => CUSTOMERS),
  createCustomer: vi.fn().mockResolvedValue("new"),
  updateCustomer: vi.fn().mockResolvedValue(undefined),
  archiveCustomer: vi.fn().mockResolvedValue(undefined),
  deleteCustomer: vi.fn(),
}));

const base = {
  street: "",
  zip: "",
  country: "DE",
  vat_id: "",
  email: "",
  phone: "",
  notes: "",
  language: "de",
  archived: false,
};

const CUSTOMERS: Customer[] = [
  { ...base, id: "c1", name: "Club Beispiel", city: "Musterstadt", kind: "business" },
  { ...base, id: "c2", name: "Festival Muster", city: "Beispielort", kind: "business" },
  { ...base, id: "c3", name: "Kunde A", city: "Musterdorf", kind: "private" },
];

describe("Customers", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
    useUi.setState({ toasts: [] });
  });
  afterEach(() => vi.useRealTimers());

  it("customer_search_filters_as_you_type", async () => {
    const user = userEvent.setup();
    render(<Customers />);
    const list = await screen.findByRole("navigation", { name: "Kunden" });
    await within(list).findByText("Club Beispiel");
    await user.type(screen.getByRole("searchbox", { name: "Kunden suchen" }), "fest");
    expect(within(list).getByText("Festival Muster")).toBeInTheDocument();
    expect(within(list).queryByText("Club Beispiel")).not.toBeInTheDocument();
    expect(within(list).queryByText("Kunde A")).not.toBeInTheDocument();
  });

  it("delete_used_customer_shows_concrete_message", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
    vi.mocked(deleteCustomer).mockRejectedValue({
      code: "in_use_by_issued_invoice",
      message: "in use",
      issues: [],
    });
    render(
      <>
        <Customers />
        <Toasts />
      </>,
    );
    const list = await screen.findByRole("navigation", { name: "Kunden" });
    await user.click(await within(list).findByText("Club Beispiel"));
    await user.click(screen.getByRole("button", { name: "Löschen" }));
    const dialog = screen.getByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Löschen" }));
    const text = "Der Kunde steht auf einer ausgestellten Rechnung. Archivieren geht.";
    expect(await screen.findByRole("alert")).toHaveTextContent(text);
    await act(() => vi.advanceTimersByTimeAsync(10_000));
    expect(screen.getByRole("alert")).toHaveTextContent(text);
  });

  it("customer_form_saves_full_payload", async () => {
    const user = userEvent.setup();
    render(<Customers />);
    const list = await screen.findByRole("navigation", { name: "Kunden" });
    await user.click(await within(list).findByText("Kunde A"));
    await user.type(screen.getByLabelText("E-Mail"), "a@example.org");
    await user.selectOptions(screen.getByLabelText("Sprache der Rechnung"), "en");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(updateCustomer).toHaveBeenCalledWith({
      ...CUSTOMERS[2],
      email: "a@example.org",
      language: "en",
    });
  });
});
