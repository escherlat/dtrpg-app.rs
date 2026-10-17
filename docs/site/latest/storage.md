---
title: Storage roots
version: latest
slug: storage
---

# Storage roots

Libri keeps the original download location as the **Default** storage root.
Additional roots can be added from **Settings → Downloads**. A root must
already exist and be writable; Libri does not recreate a missing external
volume.

## Download placement

Downloads continue to use the default root unless a saved placement rule
matches. The core storage configuration supports rules for:

- publisher name;
- collection membership; and
- minimum file size in megabytes.

Rules are evaluated in order. The first matching rule selects its named root.
If the target root is unavailable or no rule matches, Libri falls back to the
Default root.

Adding or removing a root does not move existing files. To relocate existing
content, move it manually while Libri is closed, then verify the root and file
presence from the library view.
