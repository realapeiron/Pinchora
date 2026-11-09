# Architecture decision records

An ADR records a choice that shapes the system, its reason, and its tradeoffs. Write one when a decision changes component boundaries, data access policy, deployment safety, or a rule future contributors need to preserve. Add a new ADR when a decision changes; keep the older record and mark it superseded.

Use the [template](template.md) and link the new record here.

| Decision | Status |
|---|---|
| [Modular monolith and dependency direction](modular-monolith.md) | Accepted |
| [Command crate split threshold](command-crate-split.md) | Accepted |
| [Port traits require real implementations](port-traits.md) | Accepted |
| [Compile-time checked SQL adoption](compile-time-sql.md) | Accepted |
| [Host deployment and green CI lock](host-deployment.md) | Accepted |
