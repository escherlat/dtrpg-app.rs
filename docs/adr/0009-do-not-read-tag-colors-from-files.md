## Status

Accepted. Supersedes one sentence of 0008, the one saying that a tag read from a file takes the file's color when
the library has none; the rest of 0008 stands.

## Context

0008 had a tag read back from a file take the color stored with it in the file, when the library had no color for
the tag. On macOS a file's tag attribute holds each tag as a name, optionally followed by a newline and a color
number ("Red\n6").

Testing on a Mac showed the number is not the color Finder shows. A file tagged `New` held `New\n1`, which is
gray, while Finder showed the tag in green. Finder keeps a color for each tag name and displays that; the number
in a file is the color the tag had when it was put on the file, and Finder does not rewrite the files when the
color changes. A tag recolored in Finder after it was applied is read with its old color.

Finder's own list of tag colors would be the right source, but it is private and undocumented. Its usual file,
`~/Library/SyncedPreferences/com.apple.finder.plist`, does not exist on the Mac this was tested on, so it cannot be
relied on.

## Decision

Do not read a tag's color from a file. A tag read from a file has the name and no color, on every platform.

Colors are Libri's. A color set in Libri is still written to the file as the number Finder uses, since that is what
the attribute holds, but Libri does not trust what comes back.

## Consequences

- A tag created in Finder shows in Libri with no color until the user chooses one. That is wrong less often than
  showing a specific color that Finder does not show.
- A color set in Libri may not be the color Finder shows for that tag name. Finder displays its own color for a
  custom tag, and the two can differ. The documentation says so.
- Tags that already took a color from a file, before this, keep it until the user changes it; there is no way to
  tell a color the user chose from one adopted that way.
- If Finder's tag colors become readable by a supported route, this can be revisited without a change to the file
  format.
