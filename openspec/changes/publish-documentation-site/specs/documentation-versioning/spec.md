## ADDED Requirements

### Requirement: Latest documentation is the default entry point
The site SHALL display the latest documentation when a reader opens the site root or a non-versioned documentation URL.

#### Scenario: Reader opens the site root
- **WHEN** a reader opens the documentation site's root URL
- **THEN** the site displays the latest documentation version

### Requirement: Reader can select a supported documentation version
The site SHALL provide a visible version selector that lists the latest version and each supported prior version included in the published site.

#### Scenario: Reader selects an earlier version
- **WHEN** a reader selects a supported earlier version from the version selector
- **THEN** the site navigates to that version's documentation

### Requirement: Versioned pages identify their selected version
The site SHALL indicate the documentation version currently being viewed and preserve the selection when a corresponding page exists in the selected version.

#### Scenario: Reader changes version while viewing a page
- **WHEN** a reader selects another version for a page available in both versions
- **THEN** the destination displays the corresponding page and identifies the selected version

### Requirement: Documentation site follows application visual identity
The site SHALL use the application's approved visual styling and iconography for its global navigation, version selector, and primary documentation layout.

#### Scenario: Reader views site navigation
- **WHEN** a reader views a documentation page
- **THEN** the site navigation and icons match the application's documented visual identity
