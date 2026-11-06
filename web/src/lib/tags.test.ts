import { describe, expect, it } from "vitest";

import type { CustomCommand, EmbedSpec } from "./api";
import {
  LIMITS,
  actionsProblem,
  blankCommand,
  blankEmbed,
  blankResponder,
  changedKeys,
  checkCommandName,
  cooldownLabel,
  defOf,
  describeAction,
  destinationsFor,
  embedChars,
  embedIsBlank,
  finishedActions,
  placeholdersFor,
  tidyEmbed,
  tidyName,
} from "./tags";

describe("command names", () => {
  it("are tidied like the server tidies them", () => {
    expect(tidyName("  /Rules ")).toBe("rules");
    expect(tidyName("//x")).toBe("x");
  });

  it("follow the server's rule", () => {
    for (const ok of ["a", "rules", "0day", "my-cmd_2", "x".repeat(LIMITS.name_max)]) {
      expect(checkCommandName(ok).ok, ok).toBe(true);
    }
    for (const bad of ["", "-x", "has space", "dot.dot", "x".repeat(LIMITS.name_max + 1)]) {
      expect(checkCommandName(bad).ok, bad).toBe(false);
    }
    const reserved = checkCommandName("help");
    expect(reserved.ok).toBe(false);
    if (!reserved.ok) expect(reserved.message).toContain("own commands");
  });
});

describe("placeholders", () => {
  it("differ by what the message is for", () => {
    const tokens = (kind: Parameters<typeof placeholdersFor>[0]) => placeholdersFor(kind).map((p) => p.token);
    expect(tokens("command")).toContain("{args}");
    expect(tokens("level_up")).toEqual(expect.arrayContaining(["{level}", "{role}", "{user}"]));
    expect(tokens("member_join")).toContain("{count}");
    expect(tokens("keyword")).not.toContain("{args}");
  });
});

describe("where a responder can answer", () => {
  const values = (kind: Parameters<typeof destinationsFor>[0]) => destinationsFor(kind).map((d) => d.value);
  it("needs somewhere to be", () => {
    expect(values("member_join")).toEqual(["channel", "dm"]);
    expect(values("member_leave")).toEqual(["channel"]);
    expect(values("level_up")).toEqual(["current", "channel", "dm"]);
    expect(values("keyword")).toEqual(["current", "channel", "dm"]);
  });

  it("starts each kind somewhere it can answer", () => {
    for (const kind of [
      "member_join",
      "member_leave",
      "level_up",
      "role_added",
      "role_removed",
      "keyword",
    ] as const) {
      const blank = blankResponder(kind);
      expect(values(kind), kind).toContain(blank.destination);
    }
    expect(blankResponder("keyword").match_kind).toBe("contains");
  });
});

describe("embeds", () => {
  const embed = (patch: Partial<EmbedSpec>): EmbedSpec => ({ ...blankEmbed(), ...patch });

  it("count characters the way Discord does", () => {
    expect(embedChars(null)).toBe(0);
    expect(
      embedChars(
        embed({
          title: "héllo",
          description: "d",
          footer: "f",
          fields: [{ name: "ab", value: "cde", inline: true }],
        }),
      ),
    ).toBe(5 + 1 + 1 + 2 + 3);
  });

  it("are no embed when nothing is in them", () => {
    expect(embedIsBlank(blankEmbed())).toBe(true);
    expect(embedIsBlank(embed({ title: "  " }))).toBe(true);
    expect(embedIsBlank(embed({ image: "https://x.test/a.png" }))).toBe(false);
    expect(tidyEmbed(embed({ title: " " }))).toBeNull();
    expect(tidyEmbed(embed({ title: "T", description: "" }))?.description).toBeNull();
  });
});

describe("drafts", () => {
  it("know what changed", () => {
    const saved = blankCommand();
    expect(changedKeys(saved, { ...saved })).toEqual([]);
    expect(changedKeys(saved, { ...saved, name: "x", responses: ["hi"] })).toEqual(["name", "responses"]);
    expect(changedKeys(saved, { ...saved, actions: [{ kind: "give_points", amount: 1 }] })).toEqual([
      "actions",
    ]);
  });

  it("keep only what the server stores", () => {
    const item: CustomCommand = {
      ...blankCommand(),
      id: "7",
      uses: 3,
      created_by: "1",
      created_at: "2026-11-06T00:00:00Z",
      updated_at: "2026-11-06T00:00:00Z",
      name: "rules",
    };
    expect(Object.keys(defOf(item)).sort()).toEqual(Object.keys(blankCommand()).sort());
    expect(defOf(item).name).toBe("rules");
  });
});

describe("actions", () => {
  it("are unfinished until they say enough", () => {
    expect(actionsProblem([])).toBeNull();
    expect(actionsProblem([{ kind: "add_role", role_id: "" }])).toBe("Action 1 needs a role.");
    expect(
      actionsProblem([
        { kind: "add_role", role_id: "5" },
        { kind: "dm", text: "  " },
      ]),
    ).toContain("Action 2: a direct message is 1 to");
    expect(actionsProblem([{ kind: "give_points", amount: 0 }])).toBe(
      "Action 1: points go from -1000 to 1000, and not 0.",
    );
    expect(actionsProblem([{ kind: "give_points", amount: 1001 }])).not.toBeNull();
    expect(actionsProblem([{ kind: "give_points", amount: -1000 }])).toBeNull();
    expect(actionsProblem([{ kind: "react", emoji: "" }])).toBe("Action 1 needs an emoji.");
  });

  it("are left out of the preview until they are finished", () => {
    const done = { kind: "give_points", amount: 5 } as const;
    expect(finishedActions([{ kind: "toggle_role", role_id: "" }, done])).toEqual([done]);
  });
});

describe("wording", () => {
  it("says what an action does", () => {
    expect(describeAction({ kind: "add_role", role_id: "1" })).toBe("gives a role");
    expect(describeAction({ kind: "give_points", amount: 5 })).toBe("gives 5 points");
    expect(describeAction({ kind: "give_points", amount: -5 })).toBe("takes 5 points");
    expect(describeAction({ kind: "react", emoji: "🎉" })).toBe("reacts with 🎉");
  });

  it("words a cooldown in the biggest unit", () => {
    expect(cooldownLabel(0)).toBe("none");
    expect(cooldownLabel(45)).toBe("45 s");
    expect(cooldownLabel(300)).toBe("5 min");
    expect(cooldownLabel(7200)).toBe("2 h");
  });
});
