import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { setLanguage } from "../../i18n";
import type { Product } from "../../ipc/bindings/Product";
import { createProduct, updateProduct } from "../../ipc/masterdata";
import { ProductForm } from "./ProductForm";

vi.mock("../../ipc/masterdata", () => ({
  createProduct: vi.fn().mockResolvedValue("new"),
  updateProduct: vi.fn().mockResolvedValue(undefined),
}));

const SHIRT: Product = {
  id: "p1",
  brand_id: "b1",
  name: "Shirt",
  description: "Baumwolle",
  kind: "goods",
  unit: "Stück",
  price: "20",
  price_mode: "gross",
  tax: { mode: "fixed", category: { type: "standard", rate: "19" } },
  stock: "12",
  active: true,
};

const props = {
  brandId: "b1",
  currency: "EUR",
  taxRates: ["19", "7"],
  onDone: () => {},
  onCancel: () => {},
};

describe("Catalog", () => {
  beforeEach(() => {
    setLanguage("de");
    vi.clearAllMocks();
  });

  it("product_stock_only_for_goods", async () => {
    const user = userEvent.setup();
    render(<ProductForm product={SHIRT} {...props} />);
    expect(screen.getByLabelText("Bestand")).toHaveValue("12");
    await user.selectOptions(screen.getByLabelText("Art"), "service");
    expect(screen.queryByLabelText("Bestand")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(updateProduct).toHaveBeenCalledWith({ ...SHIRT, kind: "service", stock: null });
  });

  it("product_form_saves_price_and_tax", async () => {
    const user = userEvent.setup();
    render(<ProductForm product={null} {...props} />);
    await user.type(screen.getByLabelText("Name"), "Pauschale");
    await user.selectOptions(screen.getByLabelText("Art"), "service");
    await user.type(screen.getByLabelText("Preis"), "1.250,5");
    await user.selectOptions(screen.getByLabelText("Steuer"), "standard:7");
    await user.click(screen.getByRole("button", { name: "Speichern" }));
    expect(createProduct).toHaveBeenCalledWith(
      expect.objectContaining({
        brand_id: "b1",
        name: "Pauschale",
        kind: "service",
        price: "1250.5",
        tax: { mode: "fixed", category: { type: "standard", rate: "7" } },
        stock: null,
        active: true,
      }),
    );
  });

  it("product_without_price_cannot_be_saved", async () => {
    const user = userEvent.setup();
    render(<ProductForm product={null} {...props} />);
    await user.type(screen.getByLabelText("Name"), "Etwas");
    expect(screen.getByRole("button", { name: "Speichern" })).toBeDisabled();
    expect(screen.getByText("Preis fehlt")).toBeInTheDocument();
  });
});
