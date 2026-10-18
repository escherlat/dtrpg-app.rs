## ADDED Requirements

### Requirement: Items can be tagged

The library SHALL let the user attach free-text tags to an item and remove them, and SHALL keep them in the
library index.

#### Scenario: Adding a tag

- **WHEN** the user adds the tag "solo" to an item
- **THEN** the item SHALL show the tag, and the tag SHALL still be there after a restart

#### Scenario: Tags are unique ignoring case

- **WHEN** the user adds "Solo" to an item that already has "solo"
- **THEN** the item SHALL have one tag, not two

#### Scenario: A tag that cannot be written to a file is rejected

- **WHEN** the user adds a tag that is empty, longer than 64 characters, or contains a comma
- **THEN** the tag SHALL be rejected with a message, and the item SHALL be unchanged

#### Scenario: Tags survive a catalog refresh

- **WHEN** the library catalog is refreshed from the server
- **THEN** every item still in the catalog SHALL keep its tags

#### Scenario: Removing a tag

- **WHEN** the user removes a tag from an item
- **THEN** the item SHALL no longer have it, and other items with the tag SHALL keep it

#### Scenario: Filtering by tag

- **WHEN** the user filters the catalog by a tag
- **THEN** only items with that tag SHALL be listed

#### Scenario: Tagging on a platform without file-system tags

- **WHEN** the user tags an item on Windows
- **THEN** the tag SHALL be stored in the index and SHALL remain after a restart

### Requirement: Tags are projected to the files of the item

Where the platform supports file-system tags, the library SHALL write an item's tags to each of its downloaded
files.

#### Scenario: Tagging a downloaded item

- **WHEN** the user adds a tag to an item that has downloaded files, on macOS or Linux
- **THEN** each downloaded file SHALL carry the tag in its extended attributes

#### Scenario: A file is downloaded after the item was tagged

- **WHEN** a file of a tagged item finishes downloading, on macOS or Linux
- **THEN** the file SHALL carry the item's tags

#### Scenario: A platform that cannot project

- **WHEN** the platform reports no file-system tag support
- **THEN** the library SHALL NOT attempt to write tags to files, and SHALL NOT report an error

### Requirement: The index wins on reconciliation

On launch, the library SHALL compare each downloaded file's tags with the index on platforms that support
reading them, and SHALL rewrite the file to match the index.

#### Scenario: A tag edited outside the application

- **WHEN** the user removed a tag from a file in Finder while Libri was closed
- **THEN** Libri SHALL restore the tag on the file at the next launch, and SHALL tell the user it did

#### Scenario: A file with no tags where the index has some

- **WHEN** a downloaded file carries no tags and its item has tags in the index
- **THEN** Libri SHALL write the item's tags to the file

#### Scenario: An item with no tags in the index is left alone

- **WHEN** a downloaded file carries tags and its item has none in the index
- **THEN** Libri SHALL leave the file's tags as they are at launch, because the user may have tagged the file
  before using Libri's tags

#### Scenario: A file that cannot be written

- **WHEN** a file is read-only or its volume does not support extended attributes
- **THEN** Libri SHALL skip that file, keep the tags in the index, and log the file, without failing the launch
