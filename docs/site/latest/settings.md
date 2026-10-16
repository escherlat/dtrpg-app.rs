---
title: Settings
version: latest
slug: settings
---

# Settings

Settings opens as its own window, separate from the main library window, so
you can keep working in the catalog while it's open.

## Account

Sign in with your DriveThruRPG email and password, see who you're signed in
as, and log out. Signing in is what populates your catalog in the first
place.

While signed in, the page also shows when your current access token was
acquired and when it expires. The app renews an expired token on its own the
next time it talks to DriveThruRPG, so the expiry time can be in the past
without anything being wrong; the acquired time then moves forward after the
renewal. You never need to sign out just to refresh a token.

## Appearance

Pick one of six color themes, a comfortable or compact density, and a font
for each of four independent roles (body, value, label, monospace) from any
font installed on your system. A shared text-scale stepper zooms every
size-aware element in the window together. See
[Themes and appearance]({{ '/latest/themes/' | relative_url }}) for the full
theme list.

## Downloads

Where files save to, how many download (and thumbnail-load) operations run
at once, whether collections get mirrored into a `collections/` folder of
symlinks, and how many days an item stays in the sidebar's Recently Updated
section. See [Downloads]({{ '/latest/downloads/' | relative_url }}).

## File Openers

Override which application opens a given file extension, instead of
relying on your system's default. Add, edit, or remove per-extension
overrides here.

## Advanced

Cache details (size and location on disk) and maintenance actions:
opening the cache folder directly, or clearing it.

The app name, version, and build details are in the About window (the app
menu's "About Libri"), not in Settings.
