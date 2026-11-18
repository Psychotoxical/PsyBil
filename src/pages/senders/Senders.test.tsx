import { open } from "@tauri-apps/plugin-dialog";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { Brand } from "../../ipc/bindings/Brand";
import type { Business } from "../../ipc/bindings/Business";
import {
  createBrand,
  removeBrandLogo,
  setBrandLogo,
  updateBrand,
  updateBusiness,
} from "../../ipc/masterdata";
import { useUi } from "../../store/ui";
import { BrandForm } from "./BrandForm";
import { BusinessForm } from "./BusinessForm";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../ipc/masterdata", () => ({
  updateBrand: vi.fn().mockResolvedValue(undefined),
  createBrand: vi.fn().mockResolvedValue("new"),
  setBrandLogo: vi.fn().mockResolvedValue(undefined),
  removeBrandLogo: vi.fn().mockResolvedValue(undefined),
  brandLogo: vi.fn().mockResolvedValue("data:image/png;base64,AAAA"),
  updateBusiness: vi.fn().mockResolvedValue(undefined),
  createBusiness: vi.fn().mockResolvedValue(["u2", "b2"]),
}));

const BRAND: Brand = {
  id: "b1",
  business_id: "u1",
  name: "Marke",
  logo_mime: "image/png",
  color: "#8839ef",
  phone: "0123 456",
  email: "info@example.org",
  website: "example.org",
  bank_name: "Bank",
  iban: "DE00 0000 0000 0000 0000 00",
  bic: "BANKDEFF",
  pdf_template: "modern",
  invoice_prefix: "RE",
  payment_days: 14,
  intro_text: "Hallo",
  outro_text: "Danke",
  price_mode: "net",
  archived: false,
};

const BUSINESS: Business = {
  id: "u1",
  name: "Beispiel GbR",
  street: "Straße 1",
  zip: "12345",
  city: "Stadt",
  country: "DE",
  tax_number: "12/345/67890",
  vat_id: "",
  small_business: true,
  small_business_since: "2024-01-01",
  currency: "EUR",
  number_template: "{PREFIX}-{YYYY}-{NNNN}",
  storno_prefix: "ST",
  tax_rates: ["19", "7"],
  archived: false,
};

describe("Senders", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
  });

  it("brand_form_saves_full_payload", async () => {
    const user = userEvent.setup();
    const done = vi.fn();
    render(<BrandForm brand={BRAND} businessId="u1" onDone={done} onCancel={() => {}} />);
    await user.clear(screen.getByLabelText("Name"));
    await user.type(screen.getByLabelText("Name"), "Neue Marke");
    await user.clear(screen.getByLabelText("Zahlungsziel"));
    await user.type(screen.getByLabelText("Zahlungsziel"), "30");
    await user.selectOptions(screen.getByLabelText("Vorlage"), "minimal");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(updateBrand).toHaveBeenCalledWith({
      ...BRAND,
      name: "Neue Marke",
      payment_days: 30,
      pdf_template: "minimal",
    });
    expect(done).toHaveBeenCalled();
  });

  it("save_button_says_what_is_missing", async () => {
    const user = userEvent.setup();
    render(<BrandForm brand={BRAND} businessId="u1" onDone={() => {}} onCancel={() => {}} />);
    await user.clear(screen.getByLabelText("Name"));
    expect(screen.getByRole("button", { name: "Speichern" })).toBeDisabled();
    expect(screen.getByText("Name fehlt")).toBeInTheDocument();
  });

  it("logo_pick_cancel_changes_nothing", async () => {
    const user = userEvent.setup();
    vi.mocked(open).mockResolvedValue(null);
    render(<BrandForm brand={BRAND} businessId="u1" onDone={() => {}} onCancel={() => {}} />);
    const logo = await screen.findByRole("img", { name: "Logo" });
    await user.click(screen.getByRole("button", { name: "Bild wählen" }));
    expect(open).toHaveBeenCalled();
    expect(logo).toHaveAttribute("src", "data:image/png;base64,AAAA");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(setBrandLogo).not.toHaveBeenCalled();
    expect(removeBrandLogo).not.toHaveBeenCalled();
    expect(updateBrand).toHaveBeenCalledWith(BRAND);
  });

  it("new_brand_counts_as_saved_when_only_the_logo_fails", async () => {
    const user = userEvent.setup();
    vi.mocked(open).mockResolvedValue("C:/Temp/logo.gif");
    vi.mocked(setBrandLogo).mockRejectedValueOnce({
      code: "unsupported_image",
      message: "only PNG and JPEG logos",
      issues: [],
    });
    const done = vi.fn();
    render(<BrandForm brand={null} businessId="u1" onDone={done} onCancel={() => {}} />);
    await user.type(screen.getByLabelText("Name"), "Neue Marke");
    await user.click(screen.getByRole("button", { name: "Bild wählen" }));
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(createBrand).toHaveBeenCalledTimes(1);
    expect(done).toHaveBeenCalledWith("new");
    expect(useUi.getState().toasts.map((t) => t.text)).toContain(
      "Als Logo gehen nur PNG- und JPEG-Bilder.",
    );
  });

  it("logo_pick_is_saved_with_the_brand", async () => {
    const user = userEvent.setup();
    vi.mocked(open).mockResolvedValue("C:/Temp/logo.png");
    render(<BrandForm brand={BRAND} businessId="u1" onDone={() => {}} onCancel={() => {}} />);
    await user.click(screen.getByRole("button", { name: "Bild wählen" }));
    expect(screen.getByText("logo.png")).toBeInTheDocument();
    expect(setBrandLogo).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(setBrandLogo).toHaveBeenCalledWith("b1", "C:/Temp/logo.png");
  });

  it("business_form_needs_a_counter_in_the_number_template", async () => {
    const user = userEvent.setup();
    render(<BusinessForm business={BUSINESS} onDone={() => {}} onCancel={() => {}} />);
    await user.clear(screen.getByLabelText("Nummernvorlage"));
    await user.type(screen.getByLabelText("Nummernvorlage"), "RE-{{YYYY}");
    expect(screen.getByRole("button", { name: "Speichern" })).toBeDisabled();
    expect(screen.getByText("Nummernvorlage braucht {NNNN}")).toBeInTheDocument();
  });

  it("business_form_saves_tax_rates_as_decimals", async () => {
    const user = userEvent.setup();
    render(<BusinessForm business={BUSINESS} onDone={() => {}} onCancel={() => {}} />);
    const rate = screen.getByLabelText("Steuersatz 2");
    await user.clear(rate);
    await user.type(rate, "7,5");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(updateBusiness).toHaveBeenCalledWith({ ...BUSINESS, tax_rates: ["19", "7.5"] });
  });
});
