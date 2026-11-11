import { t } from "../../../i18n/dashboard";
import type {
  CommunityEngagementData,
  Counting,
  MailSettings,
  QotdSettings,
} from "../api/communityEngagement";

export function engagementSummary(
  data: CommunityEngagementData,
  qotd: QotdSettings,
  counting: Counting,
  mail: MailSettings,
) {
  return {
    qotd: qotd.channel_id ? t("Channel selected") : t("Off"),
    counting: counting.channel_id ? t("Count {count}", { count: counting.current }) : t("Off"),
    forms: data.forms.length
      ? t(data.forms.length === 1 ? "{count} application form" : "{count} application forms", {
          count: data.forms.length,
        })
      : t("No application forms yet"),
    modmail: mail.enabled
      ? t(data.threads.length === 1 ? "{count} open thread" : "{count} open threads", {
          count: data.threads.length,
        })
      : t("Off"),
  };
}
