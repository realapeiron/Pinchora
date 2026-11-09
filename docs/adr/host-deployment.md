# Host deployment and green CI lock

## Status

Accepted

## Context

The production Compose stack runs on the owner's host. A merge and image build must not start from a branch whose exact commit has not passed CI.

## Decision

Run production deployment on the host through a repository script. Require a successful CI workflow run for the exact merge branch SHA before backup, fast-forward merge, push, and build. Keep a SHA-tagged previous image for manual rollback. GitHub Actions does not deploy to production.

## Consequences

The host operator controls the deployment and can inspect a concrete CI result. Deployment depends on GitHub status access and a local backup; rollback remains a manual action.
