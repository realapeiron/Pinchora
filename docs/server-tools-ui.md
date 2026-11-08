# W7 server tools UI reference

## Overview

Access, Audit Log, delegated navigation and the public leaderboard toggle extend the established AutoMod/tickets pixel interface. Preserve that visual identity and the existing EN/TR translation flow.
This is a surface-specific implementation reference; it defines no new product identity or global design system.

## Colors

Reuse `styles/tokens.css`: panel `#141b3d`, panel edge `#2e3d75`, ink `#fff1e8`, muted `#8fa0d8`, accent `#ff6b4a`, gold `#ffd166`.
Access checkbox accents use `--accent`; row separators use `--panel-edge`. Shared keyboard focus uses a gold outline.

## Typography

Shared pixel headings use Silkscreen with monospace fallbacks; body copy uses Noto Sans with system fallbacks.
Page headings are 32px, panel titles 16px, pixel labels 12px; shared narrow-screen rules reduce page headings to 24px.
Keep translated labels and explanatory copy in the existing i18n helpers rather than introducing literal UI strings.

## Layout

`SettingsPage`/`PageHead` supplies one h1 and a lead; `Panel` supplies distinct h2 sections and suppresses a title matching the page title.
Access role rows and previews wrap; area labels have a 2.75rem minimum height. Role/area gaps and grant separators preserve the existing density.
Audit filters and before/after values use two equal columns, collapsing to one at 600px. Actions wrap and long values break within their containers.
Preserve these rules at the required 390px viewport; shared mobile rules also stack save actions and add rows.

## Elevation & Depth

Use the shared dark panel fill, colored outline and stepped corners; the shared page heading has a hard 4px text shadow.

## Shapes

The shared art-pixel unit is `--px: 4px`; panels use it for outlines and notches. Continue using `PixelButton`, `pixel-input` and `PixelToggle`.

## Components

- **Access:** drafts are keyed to the guild. Canonical sorting/deduplication prevents ordering-only dirty states. Adding rejects duplicate, managed and everyone roles and observes the API-provided maximum. Every grant requires an area before saving.
- **Access safeguards:** preview explicitly describes unsaved grants; saved state changes after a successful response. Pending saves disable grant edits. Shared `SaveBar` supplies reset/save, navigation blocking and tab-close warnings for unsaved changes; success uses `Toast`.
- **Delegated navigation:** `ServerToolsOutlet` checks the route before rendering its form. Denied routes retain the shared page heading and an Overview link; delegated Overview lists permitted pages.
- **Audit:** filters apply on form submission; Clear resets draft and applied filters. Dates use UTC and include the selected final day. CSV uses applied filters and discloses its 10,000-change/100,000-inspected-row/8 MiB/10-second limits. A search inspects at most 500 indexed rows per request and preserves an older-page cursor even if that batch has no matches.
- **Audit details:** keyboard-operable summary buttons expose expansion state and reveal field-level Before/After values. Preserve loading, error, empty-result and older-page states.
- **Public leaderboard:** enabled means `vanity_slug !== null`. Turning off preserves the last draft slug for re-enabling; an empty enabled slug remains editable and invalid. Valid slugs are 3–32 lowercase letters/digits/hyphens with alphanumeric ends. Changes require Save XP options; the displayed public link uses the saved slug.

## Do's and Don'ts

- Keep `pages/Access.tsx`, `AuditLog.tsx`, `ServerToolsOutlet.tsx`, `LevelingParity.tsx`, `styles/server-tools.css` and `lib/server-tools.ts` aligned with the shared components.
- Preserve EN/TR wording, role safety explanations, localized area names and permission-denied behavior; client gating complements server authorization.
- Review verification: native Chrome against the isolated local test server confirmed Access saving/dirty-state behavior on desktop. At 390px, Access, Audit Log and Leveling each had one h1 and zero document horizontal overflow in EN/TR; access controls, audit filters and expanded before/after values were visually inspected. The three Playwright browser cases could not launch Chromium under the execution sandbox, so native Chrome supplied the rendered checks.

## Access regression coverage

`docs/w7-guild-route-matrix.tsv` inventories all 215 explicit guild method/path pairs. A router-source comparison fails when an endpoint is added without review. The matrix checks all seven single-area grants, anonymous requests, and CSRF on all 145 writes. Integration regressions also cover revoked Discord permissions/membership, grant escalation (including saved command/responder/module activation and XP reward sync), current role hierarchy, guild isolation, secret redaction, indexed bounded scans, export limits and private-content-free audit events. Invalid audit JSON still aborts transactional writes before redaction.
