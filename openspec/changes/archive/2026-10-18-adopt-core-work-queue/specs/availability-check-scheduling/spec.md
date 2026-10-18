## ADDED Requirements

### Requirement: Availability checks MUST run one at a time
The app MUST run at most one item availability check at once, so a batch of checks cannot hit the API in parallel.

#### Scenario: A batch of checks runs in turn
- **WHEN** several availability checks are requested together
- **THEN** one runs, and each of the others starts only after the one before it has finished

### Requirement: Availability checks MUST NOT count against the download and thumbnail limit
Availability checks MUST be scheduled independently of `max_concurrent_downloads`, so that a full set of running downloads and thumbnail fetches never holds up a check.

#### Scenario: Downloads fill the limit
- **WHEN** `max_concurrent_downloads` downloads are running and an availability check is requested
- **THEN** the check starts without waiting for a download to finish

### Requirement: Checks the user asks for MUST run ahead of automatic ones
Opening an entry's details, and choosing "Check Item Availability", MUST schedule their checks ahead of any automatic background check still waiting. The periodic timer's checks MUST be scheduled as background work. A check already running MUST NOT be interrupted.

#### Scenario: The user opens an entry while a batch is waiting
- **WHEN** the user opens an entry's details while the timer's batch of checks is waiting behind a running check
- **THEN** that entry's check runs next, ahead of the rest of the batch

#### Scenario: The user asks for a batch while background checks are waiting
- **WHEN** the user chooses "Check Item Availability" while automatic checks are waiting
- **THEN** the checks it selects run before the waiting automatic ones

#### Scenario: A waiting background check is upgraded
- **WHEN** an entry has a background check waiting and the user asks for a check of that entry
- **THEN** the waiting check is replaced by the user's, which runs at the user's priority, and the entry is checked once

#### Scenario: A background request does not displace a waiting check
- **WHEN** an automatic check is requested for an entry that already has a check waiting
- **THEN** the waiting check stays where it is, and no second check is queued

#### Scenario: A running check is not repeated
- **WHEN** a check is requested for an entry whose check is already running
- **THEN** no second check is queued

#### Scenario: Both kinds of batch share one cooldown
- **WHEN** a batch was requested, automatically or by the user, within the batch cooldown
- **THEN** a further batch request, automatic or by the user, selects no checks

### Requirement: An entry MUST show as being checked only while its check runs
The catalog MUST show an entry's checking indicator from the moment its check starts until it finishes, and MUST NOT show it while the check is only waiting.

#### Scenario: A check is waiting
- **WHEN** an entry's check is waiting behind another
- **THEN** the entry shows no checking indicator

#### Scenario: A check runs and finishes
- **WHEN** an entry's check starts, and later finishes
- **THEN** the entry shows the checking indicator from its start until it finishes
