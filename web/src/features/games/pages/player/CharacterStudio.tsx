import { t } from "../../../../i18n";
import { PageHead } from "../../../../components/PageHead";
import { Loading, Problem } from "../../../../components/States";
import { usePlayerStudio, type CharacterData } from "../../api/webPlayer";
import { Editor } from "./character/Editor";

export function CharacterStudio() {
  const query = usePlayerStudio<CharacterData>("character");
  return (
    <>
      <PageHead
        title={t("Character")}
        lead={t("A look for every adventure. Nothing changes until you save.")}
      />
      {query.data ? (
        <Editor data={query.data} />
      ) : query.isPending ? (
        <Loading />
      ) : (
        <Problem error={query.error ?? new Error(t("Could not load your collection. Try again."))} />
      )}
      {query.data && query.isError && (
        <p role="alert" className="notice notice-heart">
          {query.error.message}
        </p>
      )}
    </>
  );
}
