# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### 2026-06-13

#### Added

- Wired the storage pool manager into the GUI so pool listing, creation, and
  volume management are usable from the interface.
- Wired the SR-IOV manager into the GUI for virtual function discovery and
  assignment.
- Wired the firewall manager into the GUI, exposing rule inspection and the
  firewall analysis flow.
- Wired the networking managers (libvirt networks, the network monitor, and the
  Arch systemd-networkd integration) into `NetworkingGui`.

#### Changed

- Migrated the egui/eframe GUI off all `0.34` deprecated APIs:
  - `Frame::none()` → `Frame::new()`
  - `.rounding(...)` → `.corner_radius(...)`
  - `Context::style()` / `set_style()` → `global_style()` / `set_global_style()`
  - `ComboBox::from_id_source` and `*::id_source` → `from_id_salt` / `id_salt`
  - `TopBottomPanel` / `SidePanel` / `CentralPanel::show` →
    `Panel` constructors with `show_inside`
  - `egui::menu::bar` → `egui::MenuBar::new().ui(...)`
  - `DragValue::clamp_range` → `range`, `screen_rect` → `content_rect`,
    `ui.close_menu()` → `ui.close()`
- Refactored several types toward idiomatic Rust: derived `Default` for enums
  and config structs, implemented `FromStr` for `NovaConfig`, and replaced
  manual default-then-reassign blocks with struct literals.

#### Fixed

- Eliminated all compiler warnings from `cargo check`.
- Cleared every `cargo clippy --all-targets --all-features` lint (441 → 0),
  preferring real fixes over blanket `allow` attributes. The few retained
  `allow`s are documented at their call sites where the lint's suggestion would
  be unsound or unnecessary churn (`await_holding_lock` for synchronous
  `block_on` on the UI thread, `too_many_arguments` on an internal alert
  constructor, and one `unnecessary_unwrap` guarding a borrow-checker
  limitation).

#### Verification

- `cargo fmt --all`, `cargo check`, `cargo clippy --all-targets --all-features`,
  and `cargo audit` all pass clean.
- `cargo test --all-features`: 90 tests passing.
