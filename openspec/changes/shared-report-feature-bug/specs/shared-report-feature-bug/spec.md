## Purpose

Lets users file a bug report or feature request from within the Rust (gpui)
app, with diagnostic context attached automatically, without having to locate
the project's issue tracker themselves. Implements the parent
`shared-report-feature-bug` capability defined in `dtrpg-app/openspec`.

## ADDED Requirements

### Requirement: Report entry points
The app SHALL expose a "Report a Feature/Bug" action from the Help menu and
from the title bar account dropdown menu, both opening the same report
dialog.

#### Scenario: Opening from the Help menu
- **WHEN** the user selects "Report a Feature/Bug" from the Help menu
- **THEN** the report dialog opens

#### Scenario: Opening from the account dropdown
- **WHEN** the user selects "Report a Feature/Bug" from the title bar account
  dropdown
- **THEN** the report dialog opens

#### Scenario: Repeated invocation does not stack dialogs
- **WHEN** the report dialog is already open and the user triggers either
  entry point again
- **THEN** the existing dialog remains open rather than opening a second
  instance

### Requirement: Report dialog fields
The report dialog SHALL collect a kind selection (Bug or Feature Request), a
required subject, and a required description, and SHALL display read-only
diagnostic context consisting of the app version, commit + build date, and
target triple.

#### Scenario: Default kind selection
- **WHEN** the dialog opens
- **THEN** Bug is preselected and the user can switch it to Feature Request

#### Scenario: Diagnostics are read-only and pre-filled
- **WHEN** the dialog opens
- **THEN** the app version, commit + build date, and target triple are shown
  without requiring user input

### Requirement: Report submission validation
The dialog's submit action SHALL remain disabled until both the subject and
description contain non-whitespace content.

#### Scenario: Blank fields block submission
- **WHEN** the subject or description field is empty or contains only
  whitespace
- **THEN** the submit action is disabled

#### Scenario: Filled fields enable submission
- **WHEN** both the subject and description contain non-whitespace content
- **THEN** the submit action is enabled

### Requirement: Report submission via browser handoff
On submit, the app SHALL compose a pre-filled "new issue" URL for
`pilgrimagesoftware/dtrpg-app.rs`, embedding the kind, subject, description,
and diagnostic context in the issue title and body, and SHALL open that URL
in the user's default browser. No report content SHALL be sent over a
background network call or require stored credentials.

#### Scenario: Successful submission opens the browser
- **WHEN** the user submits a valid report
- **THEN** the app opens the user's default browser to a pre-filled GitHub
  "new issue" page containing the subject, description, kind, and diagnostic
  context

#### Scenario: Submission requires no credentials
- **WHEN** the user submits a report
- **THEN** the app does not require any stored API token, signed-in GitHub
  session, or local CLI tool to complete the submission
