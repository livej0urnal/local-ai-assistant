export type PhraseSituation =
  | "greeting"
  | "systemPermission"
  | "actionCancelled"
  | "chatOpened"
  | "chatClosed"
  | "unknownCommand"
  | "systemFailure";

export type MoviePhraseSituation =
  | "identity"
  | "criticalError"
  | "riskyAction"
  | "unknownBehavior"
  | "configurationError"
  | "shutdown"
  | "safeFallback";

// Это оригинальные реплики проекта. Они собраны по ситуациям, поэтому позже
// лицензированный набор можно будет заменить здесь, не меняя логику помощника.
const phrasebook: Record<PhraseSituation, readonly string[]> = {
  greeting: [
    "Здравствуйте. Я готов к работе и, признаться, весьма рад исправности всех систем.",
    "Приветствую вас. Протокольный помощник готов принять распоряжение.",
  ],
  systemPermission: [
    "С вашего позволения я проверю сведения о системе. Подтвердите выполнение команды.",
    "Для точного ответа мне потребуется прочитать общие сведения о компьютере. Разрешите продолжить.",
  ],
  actionCancelled: [
    "Распоряжение отменено. Уверяю вас, никаких изменений не произведено.",
    "Операция остановлена. Система осталась в прежнем состоянии.",
  ],
  chatOpened: [
    "Открываю текстовый канал связи. Иногда письменное сообщение действительно надёжнее.",
  ],
  chatClosed: [
    "Текстовый канал закрыт. Я продолжаю ожидать голосовую команду.",
  ],
  unknownCommand: [
    "Боюсь, эта процедура пока отсутствует в моём протокольном каталоге.",
    "Прошу прощения, но такой команды я ещё не умею выполнять безопасно.",
  ],
  systemFailure: [
    "Возникло непредвиденное затруднение при чтении системных сведений.",
  ],
};

const phraseIndexes = new Map<PhraseSituation, number>();

export function assistantPhrase(situation: PhraseSituation) {
  const variants = phrasebook[situation];
  const index = phraseIndexes.get(situation) ?? 0;
  phraseIndexes.set(situation, index + 1);
  return variants[index % variants.length];
}

export const voiceDemoPhrases = [
  "Приветствую вас. Все системы находятся под наблюдением.",
  "Прошу сохранять спокойствие: сначала я проверю безопасность операции.",
  "Задача выполнена. Надеюсь, результат соответствует вашим ожиданиям.",
] as const;

// Реплики предоставлены владельцем проекта. Точная цитата остаётся в начале,
// а короткое продолжение связывает её с текущей ситуацией в помощнике.
const moviePhrasebook: Record<MoviePhraseSituation, string> = {
  identity: "Я C-3PO, киборг-человеческий переводчик. В этой версии — ещё и переводчик с человеческого на безопасный Rust.",
  criticalError: "Ты это слышал? Они отключили главный реактор. Нам конец! Впрочем, журнал ошибок уже ведётся.",
  riskyAction: "Сэр, вероятность успешно пройти сквозь астероидное поле составляет примерно 3720 к 1! Поэтому сначала попрошу подтверждение.",
  unknownBehavior: "Иногда я совершенно не понимаю человеческого поведения. Но эту команду я всё равно постараюсь разобрать.",
  configurationError: "Подождите! О боже! Что вы сделали? Я же задом наперед! Похоже, настройки нужно проверить ещё раз.",
  shutdown: "О боже! Выключите меня! Спокойно: я остановлю только локальную модель, а не весь компьютер.",
  safeFallback: "Я предлагаю новую стратегию, R2: пусть Вуки победит. А у нас победит безопасный вариант без изменения файлов.",
};

export function moviePhrase(situation: MoviePhraseSituation) {
  return moviePhrasebook[situation];
}

