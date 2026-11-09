import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "../../../lib/api";
import { useDiscord } from "../../../lib/queries";
import { useWorkflows, payload, fingerprint, type Definition, type Data, type Run } from "../api/workflows";
export function useWorkflowEditor(guild: string, data: Data) {
  const [editing, setEditing] = useState<Definition | null>(null);
  const [id, setId] = useState<string | undefined>();
  const [saved, setSaved] = useState("");
  const [notice, setNotice] = useState("");
  const { save, remove, test, path } = useWorkflows(guild);
  const discord = useDiscord(guild);
  const [fake, setFake] = useState({
    user_id: "1",
    channel_id: "1",
    message_id: "1",
    value: "",
    roles: [] as string[],
    level: 1,
    account_age_days: 30,
    roll: 0,
  });
  const [fakeVars, setFakeVars] = useState("");
  const [cooldowns, setCooldowns] = useState("");
  const history = useQuery({
    queryKey: ["workflow-runs", guild, id],
    enabled: Boolean(id),
    queryFn: () => api<{ items: Run[] }>(`${path}/${id}/runs`),
  });
  const open = (def: Definition, nextId?: string) => {
    setEditing(payload(def));
    setId(nextId);
    setSaved(nextId ? fingerprint(def) : "");
    setNotice("");
    test.reset();
    save.reset();
  };
  const changed = editing && fingerprint(editing) !== saved;
  const locked = editing?.trigger.kind === "message_keyword" && !data.message_content;
  return {
    editing,
    setEditing,
    id,
    setId,
    saved,
    setSaved,
    notice,
    setNotice,
    save,
    remove,
    test,
    path,
    discord,
    fake,
    setFake,
    fakeVars,
    setFakeVars,
    cooldowns,
    setCooldowns,
    history,
    open,
    changed,
    locked,
  };
}
export type Editor = ReturnType<typeof useWorkflowEditor>;
