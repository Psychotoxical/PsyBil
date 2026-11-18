# Changelog

Every version has its own section. The section of a version becomes the text of its release draft. Changes that are not released yet collect under `## [Unreleased]`. The main → next workflow raises the version number and turns `[Unreleased]` into the next version's section; after the release, main gets that section through a pull request.

## [Unreleased]

## [1.0.0] - 2026-11-18

### Added

- Invoices for one or more businesses, each with its own brands (logo, colour, PDF template) and gap-free numbers per year.
- A guided assistant for new invoices with the real PDF growing beside it, and a check of the required details before issuing.
- Issued invoices are frozen: payments, also partial ones, set the status; a cancellation and a replacement invoice correct a mistake and take over what was paid.
- PDF in three templates (Classic, Modern, Minimal), archived as PDF/A when an invoice is issued.
- The small-business rule (§ 19 UStG): no VAT on the invoice, the required note, and a meter for last year's and this year's limits.
- An overview with open and overdue amounts, who owes what since when, and payments per month.
- A year summary per sender as PDF and CSV, and the invoice list as CSV.
- Customers and a catalogue with stock that goes down when an invoice is issued and back with a cancellation.
- Backups once a day at start and whenever you want; restoring checks the file first and backs up the current state.
- A shared data folder, for example in Nextcloud: one computer writes, the others read, and changes made on both sides are merged.
- Taking over data from VibeBill, checked invoice by invoice; the old file is only read.
- English and German, light and dark, keyboard use throughout, and help by task (F1).

