# Feishu Loc Studio

Desktop tool for game localization spreadsheets hosted in Feishu.

Current MVP flow:

- paste a Feishu spreadsheet link
- read headers and preview rows from the target sheet
- lock `Key` and `Chinese` by default, with extra user-selected locked columns
- auto-generate glossary candidates with AI
- edit glossary entries before full translation
- translate selected target columns and write results back to Feishu
- keep a local UI snapshot for quick rollback

## Current integration status

This build now uses real Feishu and OpenAI HTTP calls in the Tauri backend:

- Feishu tenant token
- spreadsheet metainfo
- sheet value read
- batch value write
- glossary generation with OpenAI structured JSON
- translation batches with OpenAI structured JSON

The local rollback is still UI-side only. Remote Feishu rollback is not implemented yet.

## Feishu app setup

Create your own Feishu custom app (self-built app) and grant it spreadsheet
read/write permission for the documents you want to edit. Then open the app,
go to connection settings, and enter your own:

- `App ID`
- `App Secret`

Credentials are stored only in the local per-user config file
(`%APPDATA%/FeishuLocStudio/config.json`) and are never bundled with or
committed to the repository. No credentials ship with this source tree.
## Run

```bash
npm install
npm run tauri dev
```

## PackyAPI Gemini

Use the `PackyAPI` preset (OpenAI-compatible), base URL `https://www.packyapi.ai/v1`, and model
`gemini-3.8-flash`. Requests use `/v1/chat/completions` and automatically send
`reasoning_effort: "high"` for this model, including streaming requests.
Enter the API key in connection settings; it is stored in the existing per-user
`%APPDATA%/FeishuLocStudio/config.json`, never in the application bundle.
The model name is the PackyAPI route identifier.

Regression tests: `cargo test --manifest-path src-tauri/Cargo.toml`.

Optional live test (uses the local credential and makes two billable requests):
`cargo test --manifest-path src-tauri/Cargo.toml live_packy_structured_output_in_both_modes -- --ignored`.

Requests have a 20-second connection timeout, a 120-second read-idle timeout,
and a 15-minute total timeout. Stopping translation drops the current AI request.
Failed rows can be retried separately. Empty, truncated, or incomplete structured
translation results are treated as failures.

Translation write-back sends only changed cells, compares the current sheet
against the preview before each write batch, and reads back each batch to verify it.
These checks detect conflicts but are not an atomic compare-and-swap transaction:
simultaneous edits between the check and write cannot be completely prevented.
If a write or verification fails, earlier batches may already be saved; reload the
sheet before retrying. Stopping during a write lets that batch finish verification.
This verification flow applies to translation write-back; glossary editing retains
its existing separate write flow.

## Desktop build

```bash
npm run tauri build
```

## Main entry points

- `src/App.tsx`
- `src/App.css`
- `src-tauri/src/lib.rs`

## License

MIT. See `LICENSE`.