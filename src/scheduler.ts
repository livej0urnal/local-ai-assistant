export type ScheduledTaskStatus = "pending" | "running" | "waiting" | "completed" | "failed" | "cancelled";
export type ScheduledTaskKind = "reminder" | "assistant" | "approval";

export type ScheduledTask = {
  id: string;
  title: string;
  prompt: string;
  runAt: string;
  kind: ScheduledTaskKind;
  status: ScheduledTaskStatus;
  progress: number;
  result: string;
  createdAt: string;
  updatedAt: string;
  profileId: string;
};

export type ScheduledTaskDraft = Pick<ScheduledTask, "title" | "prompt" | "runAt" | "kind">;

export function parseScheduledTaskCommand(text: string, currentDate = new Date()): ScheduledTaskDraft | null {
  const normalized = text.trim();
  if (!/(?:напомни|запланируй|поставь\s+задачу|сделай|выполни|проверь|проанализируй|найди|отправь|удали|перемести|переименуй|напиши|создай|remind|schedule)/iu.test(normalized)) {
    return null;
  }

  const now = new Date(currentDate);
  let runAt: Date | null = null;
  const duration = normalized.match(/через\s+(\d{1,4})\s*(минут\w*|час\w*|д(?:ень|ня|ней)|дн\w*)/iu);
  if (duration) {
    const amount = Number(duration[1]);
    const unit = duration[2].toLowerCase();
    const milliseconds = /минут/u.test(unit)
      ? amount * 60_000
      : /час/u.test(unit)
        ? amount * 3_600_000
        : amount * 86_400_000;
    runAt = new Date(now.getTime() + milliseconds);
  }

  const absolute = normalized.match(/(?:^|\s)(сегодня|завтра)(?:\s+в)?\s+(\d{1,2})(?::(\d{2}))?/iu);
  if (!runAt && absolute) {
    const hour = Number(absolute[2]);
    const minute = Number(absolute[3] || 0);
    if (hour > 23 || minute > 59) return null;
    runAt = new Date(now);
    runAt.setSeconds(0, 0);
    runAt.setHours(hour, minute, 0, 0);
    if (absolute[1].toLowerCase() === "завтра") runAt.setDate(runAt.getDate() + 1);
    if (runAt <= now) return null;
  }

  if (!runAt && /(?:^|\s)позже(?:\s|$)/iu.test(normalized)) runAt = new Date(now.getTime() + 3_600_000);
  if (!runAt) return null;

  const reminder = /(?:^|\s)напомни(?:\s+мне)?(?:\s|$)|\bremind\b/iu.test(normalized);
  const prompt = normalized
    .replace(duration?.[0] || /$^/u, " ")
    .replace(absolute?.[0] || /$^/u, " ")
    .replace(/(?:^|\s)позже(?:\s|$)/giu, " ")
    .replace(/^(?:c-?3po[,\s]*)?/iu, "")
    .replace(/^(?:напомни(?:\s+мне)?|запланируй|поставь\s+задачу)\s*/iu, "")
    .replace(/\s+/gu, " ")
    .trim();
  if (!prompt) return null;
  const requiresApproval = /(?:отправ|удал|перемест|переимен|введи|напиши\s+(?:в|контакту)|создай\s+(?:письмо|событие))/iu.test(prompt);
  return {
    title: prompt.slice(0, 80),
    prompt,
    runAt: runAt.toISOString(),
    kind: reminder ? "reminder" : requiresApproval ? "approval" : "assistant",
  };
}
