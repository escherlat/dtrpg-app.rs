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

### Requirement: Tags are reconciled by which side changed

On launch, and when the user reloads the library, the library SHALL compare the tags of each file on disk with
the index on platforms that support reading them, and SHALL bring them in step by which side changed since the
two last agreed (ADR 0008). It SHALL do so from the catalog saved on disk, without waiting for the network, so a
launch that is signed out or offline still shows the library's tags and reads the file's.

#### Scenario: A tag removed in Finder

- **WHEN** the user removed a tag from a file in Finder while Libri was closed, and the item's tags in the index
  did not change
- **THEN** Libri SHALL remove the tag from the item at the next launch, and SHALL tell the user it did

#### Scenario: A tag added in Finder

- **WHEN** the user added a tag to a file in Finder while Libri was closed
- **THEN** Libri SHALL add the tag to the item with no color, because the color stored in a file is not the color
  Finder shows (ADR 0009)

#### Scenario: A tag changed in Libri

- **WHEN** an item's tags changed in the index and the file's did not
- **THEN** Libri SHALL write the item's tags to the file

#### Scenario: Both changed

- **WHEN** the file's tags and the item's tags both changed since they last agreed
- **THEN** the later change SHALL win, and if the later cannot be told, the index SHALL win

#### Scenario: A file never synced

- **WHEN** a downloaded file carries tags and its item has none in the index
- **THEN** Libri SHALL read the file's tags into the item

#### Scenario: A file that comes back

- **WHEN** a file left the library and returns with no tags while its item has tags
- **THEN** Libri SHALL write the item's tags to the file, not remove them from the item

#### Scenario: The user turned a direction off

- **WHEN** writing to files, or reading from files, is turned off in settings
- **THEN** Libri SHALL NOT move tags in that direction, and SHALL move them when it is turned back on

#### Scenario: A file that cannot be written

- **WHEN** a file is read-only or its volume does not support extended attributes
- **THEN** Libri SHALL skip that file, keep the tags in the index, and log the file, without failing the launch

### Requirement: Tags have colors

A tag SHALL have an optional color, one of the seven colors Finder uses.

#### Scenario: A color on macOS

- **WHEN** a tag with a color is written to a file on macOS
- **THEN** Finder SHALL show the tag in that color

#### Scenario: Choosing a color

- **WHEN** the user right-clicks a tag, in the sidebar or on an item, and chooses a color
- **THEN** the tag SHALL have that color everywhere it is shown, and on every file of every item that has it

#### Scenario: A color on a platform that keeps none

- **WHEN** the platform keeps no colors for file tags
- **THEN** the color SHALL stay in the index and SHALL NOT be written to the file
