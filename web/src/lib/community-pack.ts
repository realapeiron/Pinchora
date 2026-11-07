import { useQuery } from "@tanstack/react-query";
import { api } from "./api";

export interface QotdSettings {
  channel_id: string | null;
  ping_role_id: string | null;
  hour: number;
  timezone: string;
  thread: boolean;
}
export interface Question {
  id: string;
  text: string;
  text_tr: string | null;
  approved: boolean;
  asked_at: string | null;
}
export interface Counting {
  channel_id: string;
  current: number;
  record: number;
  reset_on_fail: boolean;
  allow_math: boolean;
  fail_role_id: string | null;
}
export interface ApplicationForm {
  id: string;
  name: string;
  questions: string[];
  submit_channel_id: string | null;
  review_roles: string[];
  accept_role_id: string | null;
  dm_result: boolean;
  open: boolean;
  message_id: string | null;
}
export interface Submission {
  id: string;
  form_id: string;
  user_id: string;
  answers: string[];
  status: string;
  created_at: string;
}
export interface MailSettings {
  enabled: boolean;
  forum_channel_id: string | null;
  staff_roles: string[];
  greeting: string;
  close_after_hours: number;
  anonymous_staff: boolean;
  log_channel_id: string | null;
}
export interface MailThread {
  id: string;
  user_id: string;
  thread_id: string;
  status: string;
  opened_at: string;
}
export interface CommunityPackData {
  qotd: QotdSettings | null;
  questions: Question[];
  counting: Counting | null;
  forms: ApplicationForm[];
  submissions: Submission[];
  modmail: MailSettings | null;
  threads: MailThread[];
  message_content: boolean;
}
export function useCommunityPack(guild: string) {
  return useQuery({
    queryKey: ["community-pack", guild],
    queryFn: () => api<CommunityPackData>(`/api/guilds/${guild}/community-pack`),
  });
}
