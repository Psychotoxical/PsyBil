import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import { addPayment } from "../../ipc/payments";
import { today } from "../assistant/shared";
import { PaymentDialog } from "./PaymentDialog";

vi.mock("../../ipc/payments", () => ({ addPayment: vi.fn().mockResolvedValue("p1") }));

const props = { invoiceId: "i1", currency: "EUR", onDone: () => {}, onClose: () => {} };

describe("PaymentDialog", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
  });
  afterEach(() => setLanguage("de"));

  it("payment_amount_prefilled_with_open_and_parsed", async () => {
    const user = userEvent.setup();
    const first = render(<PaymentDialog {...props} open={1975} />);
    const amount = screen.getByLabelText("Betrag");
    expect(amount).toHaveValue("19,75");
    expect(screen.getByText("€")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Zahlung erfassen" }));
    expect(addPayment).toHaveBeenLastCalledWith("i1", "19.75", today(), "transfer", "");

    await user.clear(amount);
    await user.type(amount, "1.234,56");
    await user.click(screen.getByRole("button", { name: "Zahlung erfassen" }));
    expect(addPayment).toHaveBeenLastCalledWith("i1", "1234.56", today(), "transfer", "");
    first.unmount();

    setLanguage("en");
    render(<PaymentDialog {...props} open={0} />);
    const english = screen.getByLabelText("Amount");
    await user.type(english, "19.75");
    await user.selectOptions(screen.getByLabelText("Method"), "cash");
    await user.click(screen.getByRole("button", { name: "Record payment" }));
    expect(addPayment).toHaveBeenLastCalledWith("i1", "19.75", today(), "cash", "");
  });

  it("payment_refuses_fractions_of_cents", async () => {
    const user = userEvent.setup();
    render(<PaymentDialog {...props} open={0} />);
    await user.type(screen.getByLabelText("Betrag"), "12,345");
    expect(screen.getByText("Betrag ungültig")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Zahlung erfassen" })).toBeDisabled();
  });

  it("negative_open_amount_is_a_refund", () => {
    render(<PaymentDialog {...props} open={-500} />);
    expect(screen.getByLabelText("Betrag")).toHaveValue("-5,00");
    expect(screen.getByText("Negative Beträge sind Erstattungen.")).toBeInTheDocument();
  });
});
