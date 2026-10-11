## MODIFIED Requirements

### Requirement: All user-visible strings are keyed in locale YAML files
Every hardcoded user-facing string in `crates/dtrpg-ui/src/ui/views/` and the filter-label utility SHALL be replaced by a `t!("module.key")` call. The corresponding key SHALL exist in `en.yaml`.

#### Scenario: Sidebar navigation labels
- **WHEN** the sidebar renders
- **THEN** the labels "All Titles", "Recently Updated", "On This Device", "In the Cloud", "Publishers", and "Collections" are produced by `t!()` calls, not string literals

#### Scenario: Detail panel metadata labels
- **WHEN** the detail panel renders for any item
- **THEN** the field labels ("System", "Category", "Format", "Pages", "File size", "Released", "Status", "Added") are produced by `t!()` calls

#### Scenario: Catalog empty-state messages
- **WHEN** the catalog is empty or has no search matches
- **THEN** the empty-state messages are produced by `t!()` calls

#### Scenario: Action button labels
- **WHEN** any view renders buttons (Read, Download, Downloaded, Show in Finder, Load Thumbnail, Log Out, Sign In, etc.)
- **THEN** each label is produced by a `t!()` call
