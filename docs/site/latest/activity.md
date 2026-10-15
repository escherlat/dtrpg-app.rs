---
title: Activity and alerts
version: latest
slug: activity
---

# Activity and alerts

Libri surfaces background operations - catalog syncs, thumbnail loads,
downloads - through two related but distinct panels, both reachable from
the status bar at the bottom of the main window.

## Activity panel

An overlay listing in-progress and recently-completed background
operations. Entries here expire on a timer once an operation finishes, so
the panel stays focused on what's happening now or just happened.

## Alert history

A separate overlay reading a durable, capped log of past error activity -
unlike the activity panel's expiring list, entries here stick around so you
can review a failure from earlier in the session after the activity panel
has already cleared it. Each entry supports copying its error message.

## Status bar indicators

The status bar consolidates library totals, the active tab's summary, the
theme picker, an activity indicator, and a notification indicator (which
opens alert history) into one row along the bottom of the main window.
