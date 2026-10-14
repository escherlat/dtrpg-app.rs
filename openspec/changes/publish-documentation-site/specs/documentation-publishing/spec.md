## ADDED Requirements

### Requirement: Documentation site publishes to GitHub Pages
The system SHALL publish the built documentation site from `docs/site` to the repository's configured GitHub Pages location when the publishing workflow succeeds on the configured publishing branch.

#### Scenario: Publishing branch documentation change
- **WHEN** a change affecting `docs/site` or the documentation publishing workflow is pushed to the publishing branch
- **THEN** the workflow builds the site and deploys the resulting site to GitHub Pages

#### Scenario: Failed documentation build
- **WHEN** the site build fails
- **THEN** the workflow SHALL fail without deploying a partial site

### Requirement: Published site contains generated documentation assets
The system SHALL publish generated HTML and referenced static assets needed to render every selected documentation version.

#### Scenario: Reader opens a published documentation page
- **WHEN** a reader navigates to a published documentation URL
- **THEN** the page and its required styles, scripts, and images are available from the Pages deployment
