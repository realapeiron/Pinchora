import { useOutletContext } from "react-router";
import { type Overview } from "../../../lib/api";
import { useTempVoice } from "../api/tempVoice";
import { t } from "../api/tempVoiceI18n";
import { SettingsPage } from "../../../components/PageHead";
import { Loading, Problem } from "../../../components/States";
import { TempVoiceBody } from "./TempVoiceBody";
import "../../../styles/temp-voice.css";

export function TempVoice() {
  const data = useOutletContext<Overview>();
  const guild = data.guild.id;
  const query = useTempVoice(guild);
  return (
    <SettingsPage
      className="settings-page temp-voice-page"
      title={t("TEMP VOICE")}
      lead={t(
        "Members join a creator channel and get a voice room of their own, with the keys. It goes away when the last person leaves.",
      )}
    >
      {query.isPending && <Loading label={t("Reading the creators")} />}
      {query.isError && <Problem error={query.error} />}
      {query.isSuccess && <TempVoiceBody guild={guild} data={query.data} />}
    </SettingsPage>
  );
}
