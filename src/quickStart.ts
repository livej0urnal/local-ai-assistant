export type QuickStartRoute =
  | "weather"
  | "calculation"
  | "browser-search"
  | "application-open"
  | "file-search"
  | "file-analysis"
  | "mail-analysis"
  | "calendar-analysis";

export type QuickStartAction =
  | { kind: "command"; command: string; expectedRoute: QuickStartRoute }
  | { kind: "template"; template: string }
  | { kind: "system-info" }
  | { kind: "chat" }
  | { kind: "models" }
  | { kind: "permissions" }
  | { kind: "voice-test" };

export type QuickStartItem = {
  id: string;
  icon: string;
  label: string;
  description: string;
  action: QuickStartAction;
};

export const quickStartItems: readonly QuickStartItem[] = [
  {
    id: "system-info",
    icon: "⌘",
    label: "Покажи информацию о системе",
    description: "ОС, архитектура и оперативная память",
    action: { kind: "system-info" },
  },
  {
    id: "chat",
    icon: "◇",
    label: "Открой диалог",
    description: "Текстовая история без голосового ответа",
    action: { kind: "chat" },
  },
  {
    id: "weather",
    icon: "☁",
    label: "Какая сейчас погода?",
    description: "Актуальная сводка без открытия приложения",
    action: { kind: "command", command: "Какая сейчас погода?", expectedRoute: "weather" },
  },
  {
    id: "calculation",
    icon: "＋",
    label: "Посчитай 300 + 200",
    description: "Безопасный локальный калькулятор",
    action: { kind: "command", command: "Посчитай 300 + 200", expectedRoute: "calculation" },
  },
  {
    id: "browser-search",
    icon: "⌕",
    label: "Найди документацию Rust",
    description: "Поиск через Google Chrome",
    action: {
      kind: "command",
      command: "Открой Google Chrome и найди документацию Rust",
      expectedRoute: "browser-search",
    },
  },
  {
    id: "calculator",
    icon: "▦",
    label: "Открой калькулятор",
    description: "Запуск установленного приложения",
    action: { kind: "command", command: "Открой калькулятор", expectedRoute: "application-open" },
  },
  {
    id: "file-search",
    icon: "⌕",
    label: "Найди файл README.md",
    description: "Локальный поиск по имени на компьютере",
    action: { kind: "command", command: "Найди файл README.md", expectedRoute: "file-search" },
  },
  {
    id: "file-analysis",
    icon: "▤",
    label: "Проанализируй файл README.md",
    description: "Локальное чтение и анализ через R2-D2",
    action: { kind: "command", command: "Проанализируй файл README.md", expectedRoute: "file-analysis" },
  },
  {
    id: "file-move",
    icon: "↗",
    label: "Переместить файл",
    description: "Обратимое перемещение между папками",
    action: { kind: "template", template: "Перемести файл имя.txt в папку Документы" },
  },
  {
    id: "telegram",
    icon: "✈",
    label: "Открой Telegram",
    description: "Отправка сообщений требует подтверждения",
    action: { kind: "command", command: "Открой Telegram", expectedRoute: "application-open" },
  },
  {
    id: "mail-analysis",
    icon: "✉",
    label: "Проанализируй почту",
    description: "Приоритеты и рекомендации по письмам",
    action: {
      kind: "command",
      command: "Проанализируй почту и расставь приоритеты",
      expectedRoute: "mail-analysis",
    },
  },
  {
    id: "calendar-analysis",
    icon: "▤",
    label: "Проанализируй календарь",
    description: "Загруженность, пересечения и свободные окна",
    action: {
      kind: "command",
      command: "Проанализируй календарь и мою загруженность на неделю",
      expectedRoute: "calendar-analysis",
    },
  },
  {
    id: "email-draft",
    icon: "✎",
    label: "Составить письмо",
    description: "K-2SO подготовит текст перед подтверждением",
    action: {
      kind: "template",
      template: "Составь письмо для user@example.com тема Встреча о том, что ",
    },
  },
  {
    id: "calendar-event",
    icon: "＋",
    label: "Добавить событие",
    description: "Откроет безопасный шаблон команды",
    action: {
      kind: "template",
      template: "Создай событие Планирование завтра с 10:00 до 11:00",
    },
  },
  {
    id: "models",
    icon: "◈",
    label: "Локальные модели",
    description: "R2-D2, BB-8 и K-2SO",
    action: { kind: "models" },
  },
  {
    id: "permissions",
    icon: "◎",
    label: "Проверить доступы",
    description: "Микрофон, приложения, почта и календарь",
    action: { kind: "permissions" },
  },
  {
    id: "voice-test",
    icon: "♫",
    label: "Проверить голос",
    description: "Короткая тестовая фраза текущим голосом",
    action: { kind: "voice-test" },
  },
] as const;

export function classifyQuickStartCommand(command: string): QuickStartRoute | null {
  const normalized = command.toLocaleLowerCase("ru-RU");
  if (/погод|weather/u.test(normalized)) return "weather";
  if (/^(?:посчитай|вычисли|сколько\s+будет|calculate)(?:\s|$)/u.test(normalized)) return "calculation";
  if (/^(?:открой|запусти|open|launch)(?:\s|$)/u.test(normalized) && /(?:^|\s)(?:найди|поищи|search)(?:\s|$)/u.test(normalized)) {
    return "browser-search";
  }
  if (/^(?:найди|найти|поищи|отыщи|find|search\s+for)(?:\s|$)/u.test(normalized)) return "file-search";
  if (/^(?:проанализируй|разбери|изучи|analyze|summarize)(?:\s)+(?:файл|документ|file|document)(?:\s|$)/u.test(normalized)) return "file-analysis";
  if (/(проанализ|разбер|сортир|приоритет|важн|непрочитан).*(почт|письм|email|inbox)/u.test(normalized)) {
    return "mail-analysis";
  }
  if (/(календар|расписан|загружен|свободн.*(?:окн|врем)|встреч).*(анализ|покаж|рекоменд|загруж|свобод|недел)/u.test(normalized)) {
    return "calendar-analysis";
  }
  if (/^(?:открой|запусти|open|launch)(?:\s|$)/u.test(normalized)) return "application-open";
  return null;
}
