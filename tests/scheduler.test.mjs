import assert from "node:assert/strict";
import test from "node:test";
import { parseScheduledTaskCommand } from "../src/scheduler.ts";

const now = new Date("2026-11-11T10:00:00+01:00");

test("распознаёт относительное локальное напоминание", () => {
  const task = parseScheduledTaskCommand("Напомни через 5 минут выпить воды", now);
  assert.ok(task);
  assert.equal(task.kind, "reminder");
  assert.equal(task.prompt, "выпить воды");
  assert.equal(Date.parse(task.runAt) - now.getTime(), 5 * 60_000);
});

test("ставит безопасный анализ в фоновую очередь", () => {
  const task = parseScheduledTaskCommand("Завтра в 9:30 проанализируй почту и расставь приоритеты", now);
  assert.ok(task);
  assert.equal(task.kind, "assistant");
  assert.match(task.prompt, /проанализируй почту/iu);
  assert.equal(new Date(task.runAt).getHours(), 9);
  assert.equal(new Date(task.runAt).getMinutes(), 30);
});

test("необратимое действие оставляет подтверждение пользователю", () => {
  const task = parseScheduledTaskCommand("Через 2 часа отправь сообщение контакту Любимая", now);
  assert.ok(task);
  assert.equal(task.kind, "approval");
});

test("обычная команда без времени не становится отложенной", () => {
  assert.equal(parseScheduledTaskCommand("Проанализируй почту", now), null);
  assert.equal(parseScheduledTaskCommand("Какая погода завтра?", now), null);
});
