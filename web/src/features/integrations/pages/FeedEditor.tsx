import { useSearchParams } from "react-router";
import { Panel } from "../../../components/Panel";
import { SaveBar } from "../../../components/SaveBar";
import { SettingsRow } from "../../../components/SettingsRow";
import { t as dt } from "../../../i18n/dashboard";
import { ApiError } from "../../../lib/api";
import { blankDraft, draftOf, type Feed, type FeedDraft, type FeedKind, type FeedList } from "../api/feeds";
import { useFeedWords } from "../api/feedsWords";
import { FeedEditorSource } from "./FeedEditorSource";
import { FeedEditorFields } from "./FeedEditorFields";
import { FeedEditorPreview } from "./FeedEditorPreview";
import { feedSummary } from "./feedsSummary";
import { useFeedEditor } from "./useFeedEditor";
import { Link } from "react-router";
import { Problem } from "../../../components/States";

// /Editor

export function EditorFor({ guild, list, id }: { guild: string; list: FeedList; id: string | null }) {
  const text = useFeedWords();
  const [params] = useSearchParams();
  if (id === null) {
    const wanted = params.get("kind") as FeedKind | null;
    const kind =
      wanted && list.kinds.some((k) => k.key === wanted && k.available) ? wanted : ("youtube" as FeedKind);
    if (list.items.length >= list.max) {
      return (
        <Panel title={text("full")} tone="gold">
          <p>
            {text("this-server-already-follows-max-feeds-remove-one", { max: list.max })}{" "}
            <Link to={`/g/${guild}/feeds`}>{text("list")}</Link>
          </p>
        </Panel>
      );
    }
    return <FeedEditor key="new" guild={guild} list={list} feed={null} initial={blankDraft(kind)} />;
  }
  const feed = list.items.find((f) => f.id === id);
  if (!feed) return <Problem error={new ApiError(404, "not_found", text("this-feed-no-longer-exists"))} />;
  return <FeedEditor key={id} guild={guild} list={list} feed={feed} initial={draftOf(feed)} />;
}

function FeedEditor({
  guild,
  list,
  feed,
  initial,
}: {
  guild: string;
  list: FeedList;
  feed: Feed | null;
  initial: FeedDraft;
}) {
  const model = useFeedEditor({ guild, list, feed, initial });
  const {
    t: text,
    info,
    save,
    saved,
    changes,
    blocked,
    failure,
    missing,
    onSave,
    setDraft,
    setErrors,
    setFailure,
  } = model;
  return (
    <>
      <SettingsRow
        rowId="feed-editor-form"
        title={feed?.account ?? text("add-a-feed")}
        description={dt("Configure the post and preview it.")}
        summary={feed ? feedSummary(feed, list, info) : text("source")}
        initialOpen
      >
        <div className="editor-form settings-detail-fields">
          <FeedEditorSource model={model} />

          <FeedEditorFields model={model} />
        </div>
        <FeedEditorPreview model={model} />
      </SettingsRow>
      <SaveBar
        count={changes}
        saving={save.isPending}
        blocked={blocked}
        error={failure ?? (changes > 0 ? missing : null)}
        onSave={onSave}
        onReset={() => {
          setDraft(saved ?? initial);
          setErrors([]);
          setFailure(null);
        }}
      />
    </>
  );
}
