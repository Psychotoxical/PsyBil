import { call } from "./errors";

/** One base64 PNG per page. */
export const previewPng = (id: string, widthPx: number) =>
  call<string[]>("preview_png", { id, width_px: widthPx });
export const pdfFileName = (id: string) => call<string>("pdf_file_name", { id });
/** Writes the PDF to `path` and returns where it went. */
export const exportPdf = (id: string, path: string) => call<string>("export_pdf", { id, path });
