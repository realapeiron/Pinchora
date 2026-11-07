import { translate as spaTranslate } from "../i18n";
// The level role sync job a page is following, kept across a reload.

const key = (guild: string) => `pinchora:sync-job:${guild}`;

/** The job this browser started last for the server, if any. Storage can be blocked: then none. */
export function rememberedJob(guild: string): string | null {
  try {
    return window.localStorage.getItem(key(guild));
  } catch {
    return null;
  }
}

export function rememberJob(guild: string, id: string | null): void {
  try {
    if (id) window.localStorage.setItem(key(guild), id);
    else window.localStorage.removeItem(key(guild));
  } catch {
    // Without storage the job is just not followed after a reload.
  }
}

/** How full the progress bar is, 0 to 1: a finished job is full even when it had nobody to check. */
export function jobProgress(job: { status: string; checked: number; total: number } | undefined): number {
  if (!job) return 0;
  if (job.status === "done") return 1;
  return job.total > 0 ? Math.min(1, job.checked / job.total) : 0;
}

/** What to say while a job runs: nothing is known until the first count comes in. */
export function jobText(job: { checked: number; total: number } | undefined): string {
  if (!job || job.total === 0) return spaTranslate("Looking for members with XP…");
  return spaTranslate("Checked {arg1} of {arg2} members", { arg1: job.checked, arg2: job.total });
}
