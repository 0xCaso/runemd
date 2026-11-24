# Publish readiness and open risks (2026-11-24)

Answers G1 and G2 of `docs/feedback/2026-11-24-publish-ui-review.md`. Branch `fix/fire-off-ui-polish`.

## G1: what is missing to publish

| # | Gap | Evidence | Needs |
|---|---|---|---|
| 1 | **Signing and notarization.** The app is signed with the self-made "RuneMD Local" certificate; Gatekeeper rejects it, so on another Mac it does not open with a double click. | `scripts/make_app.sh:15-17`; `spctl -a -vv build/RuneMD.app` → `rejected`, `TeamIdentifier=not set` | Apple Developer Program account (Matteo), Developer ID Application certificate, `notarytool` + `stapler` in `make_app.sh` |
| 2 | **Release package.** No DMG or zip, no git tag, no GitHub Release; version `0.0.1`. | `Cargo.toml:3`, `git tag` empty | A `make release` target: build, sign, notarize, zip/DMG, tag |
| 3 | **Where people download it.** The repo is private; there is no download page. | `gh repo view` → `PRIVATE` | Matteo: public repo or a site, and when |
| 4 | **Updates.** No update check: users stay on the version they downloaded. | no updater in `Cargo.toml` or `src` | v1 can ship without one; a simple "new version on GitHub Releases" check later |
| 5 | **Crash trace.** No panic hook: a panic closes the window with no log for the user to send. Save errors already keep a recovery copy. | no `panic::set_hook` in `src`; `src/edit/recovery.rs` | Done: `src/crash.rs` writes `~/Library/Logs/RuneMD/panic-<stamp>.log` |
| 6 | **Finder "Open With".** Info.plist claims only `.md` / `.markdown`, but the app also reads JSON and text. | `assets/Info.plist:19-20` | Add `public.json` and `public.plain-text` as Viewer (Alternate rank), if wanted |
| 7 | **Bundle id.** `dev.matteo.runemd` is personal; changing it after release loses users' Launch Services links. The prefs folder is `RuneMD/`, not tied to the id. | `assets/Info.plist:4`, `src/settings.rs:2` | Done: `io.github.0xcaso.runemd` (`docs/decisions/2026-11-24-bundle-id.md`) |
| 8 | **Audience.** macOS 26 or later only. | `assets/Info.plist:12` | Conscious choice; say it on the download page |

Already in place: MIT `LICENSE`, user `README.md`, recovery copies, startup p50 293–297 ms against a 350 ms budget (`docs/perf.md:13-14`).

## G2: what else might be broken

- **Tests:** `make check` green, 650 passed (2026-11-24, after 456974d).
- **Perf measures** (the 9 ignored tests, `cargo test --profile perf -- --ignored`), all inside their budgets: source typing p95 1.4 ms (budget 8 ms), Quick Open p95 1.1 ms, Find UI thread p95 3.4 ms, reload of a 1 MB append p95 6.2 ms, sidebar refresh p95 ≤ 0.15 ms. One outlier: Find "results + frame" max 16.9 ms (one frame at 60 Hz), p95 is 3.4 ms.
- **Seen in the running app** (virtual screen, dark and light):
  - The empty window still shows "⌘E source" and the ↔ width button, with no document to act on.
  - "Charged edge on warnings" has no visible off state: with no check mark it does not read as a switch.
  - In the Fire group the "Fire at the bottom of the panel" line sits right on the Off row, with less space than the other groups.
- **Fixed in this branch:** watched-folder path cut at the end (now middle), fire choices not starting with the default (now Off first).
