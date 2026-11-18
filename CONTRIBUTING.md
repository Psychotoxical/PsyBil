# Contributing to PsyBil

Thanks for helping. Bug reports, ideas, translations and code are all welcome.

## Issues

Open an issue with one of the templates: bug report or feature request. For a bug, name the PsyBil version, the system and the package (installer, AppImage, deb, rpm, DMG) and the steps that lead to it. Security problems go through [a private report](https://github.com/Psychotoxical/PsyBil/security/advisories/new), not a public issue.

PsyBil handles invoices. Never attach a real invoice, customer data, bank details or a database file to an issue; describe the case with made-up names and amounts instead.

## Translations

Visible texts are written in English, and the English text is the key. Every other language is one JSON file in `locales/` (`de.json` today). A pull request that only touches a language file is fine; a missing text shows in English.

## Code

1. Set up the project as in [Building from source](README.md#building-from-source) (`npm ci`, then `npm run tauri dev`).
2. Make the change on a branch and open a pull request against `main`. Keep one topic per pull request.
3. Before you push, run what CI runs:

   ```
   npm run check
   npm test
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```

   `cargo test` also writes the TypeScript types into `src/ipc/bindings`; commit them when they change.

### House rules

- **Changelog:** every pull request adds at least one line under `## [Unreleased]` in [CHANGELOG.md](CHANGELOG.md), in the section that fits (`Added`, `Changed`, `Fixed`). Write it for users: what they notice, not how it is built. CI fails without it.
- **Money and tax:** amounts are whole cents (`i64`) and every calculation happens in the Rust core (`crates/core`). The window only shows what the core returns; it never adds, rounds or converts amounts itself.
- **Issued is issued:** an issued invoice, its number and its archived PDF never change. A correction goes through a cancellation and a new invoice.
- **Texts in the interface:** a new text goes into the code in English and into `locales/de.json` in German; a test fails when a German text is missing.
- **Commands:** a new command the window calls needs its entry in `src-tauri/build.rs` (`COMMANDS`), in `generate_handler!` and in `src-tauri/capabilities/main.json`, and it must be `async`. Tests check all of that.
- **Tests:** a fix comes with a test that fails without it. Test data uses neutral placeholders ("Band", "Kunde A", files in the temp folder), never real names, addresses or invoices.

## How a pull request goes in

CI runs the changelog check, Biome, TypeScript, rustfmt, actionlint and, when code changes, the tests on Linux. The check `ci-ok` sums it up and must be green. Only maintainers merge: a maintainer reviews the pull request and squashes it into `main`. Please do not merge your own pull request, even if you have write access.

Releases are built by the maintainers: the release workflow runs the tests on Windows and macOS too, builds all packages, signs and notarizes the macOS disk image and creates a draft. Contributors do not need to change the version number.
