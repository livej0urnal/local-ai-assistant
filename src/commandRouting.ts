export type DirectoryListRequest = {
  reference: string;
  limit: number;
  filesOnly: boolean;
};

export type WakePhrase = {
  command: string;
};

// Системное распознавание записывает имя по-разному: C3PO, «си три пи о»
// или даже «с 3 по». Приветствие перед именем тоже считается обращением.
export function parseWakePhrase(text: string): WakePhrase | null {
  const wakeWord = /^\s*(?:(?:привет|эй|слушай|hello|hey)\s*[,!;:.-]*\s*)?(?:c\s*[- ]?(?:3|three)\s*[- ]?p\s*[- ]?o|(?:с|си)\s*[- ]?(?:3|три)\s*[- ]?(?:п|пи)\s*[- ]?о?|ситрипио)(?=\s|[,.:;!?-]|$)/iu;
  const match = text.match(wakeWord);
  if (!match) return null;
  return {
    command: text.slice(match[0].length).replace(/^[,.:;!?\s-]+/u, "").trim(),
  };
}

// Простые операции с ОС маршрутизируются до обращения к модели. Так список
// папки остаётся быстрым и фактическим даже при выключенной локальной LLM.
export function parseDirectoryListCommand(text: string): DirectoryListRequest | null {
  const normalized = text.trim().toLowerCase();
  if (!/(?:какие|покажи|перечисли|список|что\s+(?:есть|находится))/.test(normalized)
    || !/(?:файл|папк|содержим)/.test(normalized)) return null;

  const standardDirectory = normalized.match(
    /(?:в|из)\s+(?:папк[\p{L}]*\s+)?(документ(?:ы|ах|ов)?|загрузк(?:и|ах|у)?|рабоч(?:ий|ем)\s+стол(?:е)?|documents|downloads|desktop)(?=\s|$)/u,
  )?.[1];
  const explicitDirectory = normalized.match(/(?:в|из)\s+папк[\p{L}]*\s+[«"']?(.+?)[»"']?(?:\s+(?:дай|покажи|первые|перв))?$/u)?.[1];
  const rawReference = standardDirectory || explicitDirectory;
  if (!rawReference) return null;

  const directoryAliases: Record<string, string> = {
    документы: "документы", документах: "документы", документов: "документы", documents: "documents",
    загрузки: "загрузки", загрузках: "загрузки", загрузку: "загрузки", downloads: "downloads",
    "рабочий стол": "рабочий стол", "рабочем столе": "рабочий стол", desktop: "desktop",
  };
  const numberWords: Record<string, number> = {
    один: 1, одну: 1, первых: 1, два: 2, двух: 2, три: 3, трех: 3, трёх: 3,
    четыре: 4, четырех: 4, четырёх: 4, пять: 5, пяти: 5, десять: 10, десяти: 10,
  };
  const limitToken = normalized.match(/перв[\p{L}]*\s+(\d+|[а-яё]+)/u)?.[1];
  const parsedLimit = limitToken ? (Number.parseInt(limitToken, 10) || numberWords[limitToken]) : 20;
  return {
    reference: directoryAliases[rawReference] || rawReference.trim(),
    limit: Math.max(1, Math.min(parsedLimit || 20, 100)),
    filesOnly: /файл/.test(normalized)
      && !/(?:файлы?\s+и\s+папк|папк[\p{L}]*\s+и\s+файл)/u.test(normalized),
  };
}
