export type PhraseSituation =
  | "greeting"
  | "systemPermission"
  | "actionCancelled"
  | "chatOpened"
  | "chatClosed"
  | "unknownCommand"
  | "systemFailure";

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

