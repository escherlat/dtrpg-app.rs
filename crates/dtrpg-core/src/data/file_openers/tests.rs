//! Tests for the file-opener overrides and the saved catalog view.

use super::*;

fn entry(ext: &str, path: &str) -> FileOpenerEntry {
    FileOpenerEntry { extension: ext.to_owned(),
                      app_path:  PathBuf::from(path), }
}

fn config_with(entries: Vec<FileOpenerEntry>) -> FileOpenerConfig {
    FileOpenerConfig { entries }
}

// ── find_override
// ─────────────────────────────────────────────────────────

#[test]
fn find_override_exact_match() {
    let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
    assert_eq!(cfg.find_override("pdf"),
               Some(Path::new("/Applications/Preview.app")));
}

#[test]
fn find_override_case_insensitive() {
    let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
    assert_eq!(cfg.find_override("PDF"),
               Some(Path::new("/Applications/Preview.app")));
    assert_eq!(cfg.find_override("Pdf"),
               Some(Path::new("/Applications/Preview.app")));
}

#[test]
fn find_override_dot_prefixed_input() {
    let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
    assert_eq!(cfg.find_override(".pdf"),
               Some(Path::new("/Applications/Preview.app")));
    assert_eq!(cfg.find_override(".PDF"),
               Some(Path::new("/Applications/Preview.app")));
}

#[test]
fn find_override_no_match() {
    let cfg = config_with(vec![entry("pdf", "/Applications/Preview.app")]);
    assert_eq!(cfg.find_override("epub"), None);
    assert_eq!(cfg.find_override(""), None);
}

// ── add ───────────────────────────────────────────────────────────────────

#[test]
fn add_new_entry() {
    let mut cfg = FileOpenerConfig::default();
    let result = cfg.add(entry("pdf", "/Applications/Preview.app"));
    assert_eq!(result, AddOutcome::Added);
    assert_eq!(cfg.entries().len(), 1);
    assert_eq!(cfg.entries()[0].extension, "pdf");
}

#[test]
fn add_normalizes_extension() {
    let mut cfg = FileOpenerConfig::default();
    cfg.add(entry(".PDF", "/Applications/Preview.app"));
    assert_eq!(cfg.entries()[0].extension, "pdf");
}

#[test]
fn add_trims_whitespace_in_extension() {
    let mut cfg = FileOpenerConfig::default();
    cfg.add(entry("  pdf  ", "/Applications/Preview.app"));
    assert_eq!(cfg.entries()[0].extension, "pdf");
}

#[test]
fn add_duplicate_replaces() {
    let mut cfg = FileOpenerConfig::default();
    cfg.add(entry("pdf", "/Applications/Preview.app"));
    let result = cfg.add(entry("pdf", "/Applications/Acrobat.app"));
    assert_eq!(result, AddOutcome::Replaced);
    assert_eq!(cfg.entries().len(), 1);
    assert_eq!(cfg.entries()[0].app_path,
               PathBuf::from("/Applications/Acrobat.app"));
}

// ── validate_all
// ──────────────────────────────────────────────────────────

#[test]
fn validate_all_flags_missing_paths() {
    let cfg = config_with(vec![entry("pdf", "/nonexistent/Preview.app"),
                               entry("epub", "/nonexistent/Calibre.app"),]);
    let stale = cfg.validate_all();
    assert_eq!(stale.len(), 2);
}

#[test]
fn validate_all_accepts_existing_paths() {
    let tmp = std::env::temp_dir();
    let cfg = config_with(vec![entry("pdf", tmp.to_str().unwrap_or("/tmp"))]);
    let stale = cfg.validate_all();
    assert_eq!(stale.len(), 0);
}

// ── Grouping
// ─────────────────────────────────────────────────────────────

// A test that cannot read its own fixture should fail loudly.
#[allow(clippy::expect_used)]
fn prefs_from(toml_text: &str) -> CatalogViewPrefs {
    toml::from_str(toml_text).expect("valid catalog view preferences")
}

#[test]
#[allow(clippy::expect_used)] // A fixture that cannot be written should fail loudly.
fn every_grouping_survives_being_saved_and_loaded() {
    for group_by in [GroupBy::None, GroupBy::Publisher, GroupBy::Tag] {
        let saved = CatalogViewPrefs::from_state(&SidebarFilter::AllTitles,
                                                 SortMethod::Title,
                                                 group_by,
                                                 CatalogPresentation::Grid,
                                                 &ListColumn::DEFAULT_CHOSEN);
        let text = toml::to_string(&saved).expect("preferences serialize");

        assert_eq!(prefs_from(&text).to_group_by(), group_by);
    }
}

#[test]
fn a_catalog_saved_as_grouped_before_there_was_a_choice_is_grouped_by_publisher() {
    assert_eq!(prefs_from("grouped = true").to_group_by(),
               GroupBy::Publisher);
    assert_eq!(prefs_from("grouped = false").to_group_by(), GroupBy::None);
    assert_eq!(prefs_from("").to_group_by(), GroupBy::None);
}

#[test]
fn the_grouping_wins_over_the_old_setting_and_an_unknown_one_means_none() {
    assert_eq!(prefs_from("group_by = \"Tag\"\ngrouped = true").to_group_by(),
               GroupBy::Tag);
    assert_eq!(prefs_from("group_by = \"Planet\"").to_group_by(),
               GroupBy::None);
}

// ── List columns
// ───────────────────────────────────────────────────────────────

#[test]
#[allow(clippy::expect_used)] // A fixture that cannot be written should fail loudly.
fn the_chosen_columns_survive_being_saved_and_loaded() {
    let chosen = [ListColumn::Tags, ListColumn::Pages];
    let saved = CatalogViewPrefs::from_state(&SidebarFilter::AllTitles,
                                             SortMethod::Title,
                                             GroupBy::None,
                                             CatalogPresentation::List,
                                             &chosen);
    let text = toml::to_string(&saved).expect("preferences serialize");

    // Back in their usual order, not the order they were chosen in.
    assert_eq!(prefs_from(&text).to_list_columns(),
               [ListColumn::Pages, ListColumn::Tags]);
}

#[test]
fn no_saved_choice_means_the_default_columns_and_an_empty_one_means_none() {
    assert_eq!(prefs_from("").to_list_columns(), ListColumn::DEFAULT_CHOSEN);
    assert_eq!(prefs_from("list_columns = []").to_list_columns(), []);
}

#[test]
fn a_saved_name_that_is_not_a_choosable_column_is_ignored() {
    let prefs = prefs_from("list_columns = [\"title\", \"nonsense\", \"size\", \"size\"]");

    assert_eq!(prefs.to_list_columns(), [ListColumn::Size]);
}

#[test]
fn the_columns_shown_are_the_title_the_chosen_ones_in_order_then_status_and_open() {
    assert_eq!(ListColumn::shown(&[ListColumn::Tags, ListColumn::Publisher]),
               [ListColumn::Title,
                ListColumn::Publisher,
                ListColumn::Tags,
                ListColumn::Status,
                ListColumn::Open]);
    assert_eq!(ListColumn::shown(&[]),
               [ListColumn::Title, ListColumn::Status, ListColumn::Open]);
}

#[test]
fn every_column_is_found_by_its_key() {
    for column in ListColumn::shown(&ListColumn::CHOOSABLE) {
        assert_eq!(ListColumn::from_key(column.key()), Some(column));
    }
    assert_eq!(ListColumn::from_key("nonsense"), None);
}
