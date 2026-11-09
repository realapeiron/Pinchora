# Command crate split threshold

## Status

Accepted

## Context

Commands share `App`, `Cx`, and `Responder` types and make calls across domains. Splitting crates before boundaries are clear would create dependency cycles and more wiring.

## Decision

Organize commands into domain folders and move shared behavior into services first. Measure build time before moving code into domain crates. Split a domain into its own crate only when that measurement supports a gain of at least 20 percent.

## Consequences

The current crate remains larger while domain boundaries settle. A future split needs a recorded baseline and comparable build measurement.
