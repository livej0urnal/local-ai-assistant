import assert from "node:assert/strict";
import test from "node:test";
import { classifyQuickStartCommand, quickStartItems } from "../src/quickStart.ts";
import { parseDirectoryListCommand, parseWakePhrase } from "../src/commandRouting.ts";

test("каждая функция быстрого старта имеет уникальный идентификатор и понятное описание", () => {
  assert.equal(quickStartItems.length, 20);
  assert.equal(new Set(quickStartItems.map((item) => item.id)).size, quickStartItems.length);
  for (const item of quickStartItems) {
    assert.ok(item.label.trim().length >= 4, `${item.id}: отсутствует название`);
    assert.ok(item.description.trim().length >= 8, `${item.id}: отсутствует описание`);
    assert.ok(item.icon.trim(), `${item.id}: отсутствует значок`);
  }
});

for (const item of quickStartItems) {
  test(`быстрый старт: ${item.id}`, () => {
    if (item.action.kind === "command") {
      assert.equal(classifyQuickStartCommand(item.action.command), item.action.expectedRoute);
      return;
    }
    if (item.action.kind === "template") {
      assert.ok(item.action.template.length >= 20);
      assert.match(item.action.template, /(?:письм|событи|перемести|исправь)/iu);
      return;
    }
    assert.ok(["system-info", "chat", "models", "tasks", "permissions", "voice-test"].includes(item.action.kind));
  });
}

test("поиск в интернете не превращается в поиск локального файла", () => {
  assert.equal(classifyQuickStartCommand("Найди в интернете документацию Tauri"), "browser-search");
  assert.equal(classifyQuickStartCommand("Найди в интернете последнюю погоду"), "weather");
  assert.equal(classifyQuickStartCommand("Найди файл отчет.pdf"), "file-search");
});

test("список файлов в стандартной папке не уходит к локальной модели", () => {
  assert.deepEqual(
    parseDirectoryListCommand("какие файлы в документах дай список первых трех"),
    { reference: "документы", limit: 3, filesOnly: true },
  );
  assert.deepEqual(
    parseDirectoryListCommand("Покажи первые 2 файла в Загрузках"),
    { reference: "загрузки", limit: 2, filesOnly: true },
  );
  assert.deepEqual(
    parseDirectoryListCommand("Что находится в папке Projects"),
    { reference: "projects", limit: 20, filesOnly: false },
  );
});

test("имя C-3PO пробуждает скрытого помощника в вариантах распознавания", () => {
  assert.deepEqual(parseWakePhrase("C-3PO какая погода"), { command: "какая погода" });
  assert.deepEqual(parseWakePhrase("Привет, C3PO, открой калькулятор"), { command: "открой калькулятор" });
  assert.deepEqual(parseWakePhrase("си три пи о"), { command: "" });
  assert.deepEqual(parseWakePhrase("с 3 по покажи уведомление"), { command: "покажи уведомление" });
  assert.equal(parseWakePhrase("просто фоновый разговор"), null);
});
