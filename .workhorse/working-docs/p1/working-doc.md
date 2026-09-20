---
status: draft
---

# Make the feed model structural, and move presentation out of the wire

Rework the diagnostics wire so it carries data rather than presentation: strip `group` and ordering, fold identity into samples, make feeds device-pushed with `subscribe` as the resume path, and merge the two message enums into one.
