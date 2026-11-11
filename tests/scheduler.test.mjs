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

test("распознаёт часы, дни и команду с обращением по имени", () => {
  const inHours = parseScheduledTaskCommand("C-3PO, через 3 часа проверь календарь", now);
  const inDays = parseScheduledTaskCommand("Через 2 дня проанализируй почту", now);
  assert.ok(inHours);
  assert.ok(inDays);
  assert.equal(inHours.prompt, "проверь календарь");
  assert.equal(Date.parse(inHours.runAt) - now.getTime(), 3 * 3_600_000);
  assert.equal(Date.parse(inDays.runAt) - now.getTime(), 2 * 86_400_000);
});

test("позже означает безопасную отсрочку на один час", () => {
  const task = parseScheduledTaskCommand("Позже проверь погоду", now);
  assert.ok(task);
  assert.equal(task.kind, "assistant");
  assert.equal(task.prompt, "проверь погоду");
  assert.equal(Date.parse(task.runAt) - now.getTime(), 3_600_000);
});

test("отклоняет прошедшее и некорректное календарное время", () => {
  assert.equal(parseScheduledTaskCommand("Сегодня в 9:00 проверь почту", now), null);
  assert.equal(parseScheduledTaskCommand("Завтра в 25:10 проверь почту", now), null);
  assert.equal(parseScheduledTaskCommand("Завтра в 10:75 проверь почту", now), null);
});

test("изменение файлов, письма и календаря всегда требует подтверждения", () => {
  const commands = [
    "Через 5 минут удали файл отчёт.txt",
    "Завтра в 11:00 создай письмо для user@example.com",
    "Завтра в 12:00 создай событие Планирование",
    "Через 1 час переименуй файл notes.txt",
  ];
  for (const command of commands) {
    assert.equal(parseScheduledTaskCommand(command, now)?.kind, "approval", command);
  }
});

test("пустая задача и вопрос о будущем не создают расписание", () => {
  assert.equal(parseScheduledTaskCommand("Напомни через 5 минут", now), null);
  assert.equal(parseScheduledTaskCommand("Будет ли дождь завтра в 9:00?", now), null);
});
