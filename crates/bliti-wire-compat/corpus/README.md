# Regression corpus

One message per file, as JSON, exactly as it goes on the wire. Each file is named for what it
pins. Add one when a compatibility failure is found and is worth keeping past the change that
caused it, or when a shape matters and the generator is not reliably reaching it.

Raw JSON rather than a proptest seed: a seed replays only through the generator that found it, and
that generator lives in `bliti-core` and changes. A recorded message replays verbatim through any
build, and outlives both the generator and the baseline that produced it.

This is not where the broad machine-generated snapshot lives. `../baseline-snapshot.jsonl` holds
that, is anonymous, and goes away once the baseline can generate live. These are curated and stay.
