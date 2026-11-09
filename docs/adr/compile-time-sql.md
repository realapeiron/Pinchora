# Compile-time checked SQL adoption

## Status

Accepted

## Context

The repository has many runtime checked queries. Converting every query during domain moves would mix verification changes with structural changes.

## Decision

Use compile-time checked SQL for new queries and queries whose contents change. Queries that only move keep their existing form. Track the number of old runtime checked queries and ratchet it downward.

## Consequences

New and edited SQL gains earlier validation. Existing queries migrate gradually, and the count gate prevents regression without forcing unrelated rewrites.
