# Port traits require real implementations

## Status

Accepted

## Context

An abstraction for a single implementation adds code and hides concrete behavior without demonstrating a useful boundary.

## Decision

Introduce a port trait only when there are two real implementations of the same behavior. A mock can qualify when it is a working implementation of an external boundary, such as Discord REST.

## Consequences

Concrete dependencies remain visible until a second implementation proves the interface. Adding that implementation may require a small refactor at that time.
