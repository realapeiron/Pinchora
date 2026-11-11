import type { Case, CasesPage, Rung } from "../../../lib/api";

export function caseFromSearch(search: string): number | null {
  const raw = new URLSearchParams(search).get("case");
  if (!raw || !/^[1-9]\d*$/.test(raw)) return null;
  const number = Number(raw);
  return Number.isSafeInteger(number) ? number : null;
}

export function searchWithCase(search: string, number: number | null): string {
  const params = new URLSearchParams(search);
  if (number === null) params.delete("case");
  else params.set("case", String(number));
  return params.toString();
}

export function kindCounts(kinds: CasesPage["kinds"]): number {
  return kinds.reduce((sum, kind) => sum + kind.count, 0);
}

/** The kinds worth a tab: those with cases under the current filters, and the one already chosen. */
export function visibleKinds(kinds: CasesPage["kinds"], chosen: string): CasesPage["kinds"] {
  return kinds.filter((kind) => kind.count > 0 || kind.key === chosen);
}

export function nextWarningRung(entry: Case, history: Case[], ladder: Rung[] | undefined) {
  if (entry.kind !== "warn" || entry.on_channel || !ladder?.length) return null;
  const active = Number(entry.active) + history.filter((c) => c.kind === "warn" && c.active).length;
  const rung = [...ladder].sort((a, b) => a.warns - b.warns).find((item) => item.warns > active);
  return rung ? { active, rung } : null;
}

export function caseTone(kind: string): string {
  if (kind === "ban" || kind === "kick") return "is-danger";
  if (["unban", "untimeout", "unwarn", "unlock"].includes(kind)) return "is-good";
  if (["timeout", "warn", "purge", "lock"].includes(kind)) return "is-warn";
  return "";
}
