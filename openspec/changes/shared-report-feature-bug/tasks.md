## 1. Action and Entry Points

- [x] 1.1 Add `ReportFeatureBug` action to `actions.rs`
- [x] 1.2 Register it in the Help menu (`app/mod.rs`) below About, with an
      app-level fallback handler mirroring `About`'s
- [x] 1.3 Dispatch it from the title bar account dropdown (`title_bar_view.rs`)

## 2. Dialog

- [x] 2.1 Add `report_feature_bug_dialog.rs`: kind radio selector (Bug/Feature
      Request), subject input, description input, diagnostics block
- [x] 2.2 Guard against stacking via `window.has_active_dialog(cx)`
- [x] 2.3 Disable submit until subject and description are both non-whitespace
- [x] 2.4 Register the real handler on `LibraryRootView` (`root_view.rs`)

## 3. Submission

- [x] 3.1 Compose the kind-prefixed, percent-encoded GitHub new-issue URL
      (subject as title, description + diagnostics as body)
- [x] 3.2 Open it via `cx.open_url()`; close the dialog on submit

## 4. i18n

- [x] 4.1 Add `menu.help_report`, `title_bar.report`, and `report.*` strings
      to `en.yaml`, `de.yaml`, `fr.yaml`

## 5. Verification

- [x] 5.1 Unit tests: field-validation rule, URL target/title/body
      composition, percent-encoding, no raw space/newline in the URL
- [x] 5.2 `cargo check --workspace --all-targets`, `cargo clippy --workspace
      --all-targets -- -D warnings`, `cargo +nightly fmt --check`, `cargo test
      --workspace` all pass
