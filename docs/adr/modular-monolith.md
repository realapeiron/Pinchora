# Modular monolith and dependency direction

## Status

Accepted

## Context

Commands and web routes share game behavior and data access. Organizing behavior by domain makes ownership clearer while preserving one deployable application.

## Decision

Keep a modular monolith with domain folders and a shared services layer. Dependencies flow from commands and web to services, from services to database and cache, and from those layers to core. Domain registration keeps the application wiring explicit.

## Consequences

Shared rules have one home, and domain changes can be reviewed together. Boundary checks are needed to prevent upper layers from becoming shortcuts around services.
