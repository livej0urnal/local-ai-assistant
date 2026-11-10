export const languages = [
  { code: "ru", nativeName: "Русский", speechCode: "ru-RU", modelName: "Russian" },
  { code: "en", nativeName: "English", speechCode: "en-US", modelName: "English" },
  { code: "es", nativeName: "Español", speechCode: "es-ES", modelName: "Spanish" },
  { code: "de", nativeName: "Deutsch", speechCode: "de-DE", modelName: "German" },
  { code: "fr", nativeName: "Français", speechCode: "fr-FR", modelName: "French" },
  { code: "it", nativeName: "Italiano", speechCode: "it-IT", modelName: "Italian" },
  { code: "pt", nativeName: "Português", speechCode: "pt-BR", modelName: "Portuguese" },
  { code: "zh", nativeName: "简体中文", speechCode: "zh-CN", modelName: "Simplified Chinese" },
  { code: "ja", nativeName: "日本語", speechCode: "ja-JP", modelName: "Japanese" },
  { code: "ar", nativeName: "العربية", speechCode: "ar-SA", modelName: "Arabic" },
] as const;

export type LanguageCode = (typeof languages)[number]["code"];

type Messages = {
  localAssistant: string;
  permissions: string;
  openChat: string;
  language: string;
  languageSettings: string;
  interfaceLanguage: string;
  commandLanguage: string;
  languageHint: string;
  done: string;
  voiceMode: string;
  idle: string;
  listening: string;
  thinking: string;
  speaking: string;
  paused: string;
  error: string;
  sayCommand: string;
  pauseMicrophone: string;
  enableMicrophone: string;
  quickStart: string;
  whatCanSay: string;
  localOnly: string;
  systemCommand: string;
  systemDescription: string;
  chatCommand: string;
  chatDescription: string;
  testVoice: string;
  voiceDescription: string;
  history: string;
  dialogTitle: string;
  enterCommand: string;
  close: string;
  greeting: string;
  consultingR2: string;
};

export const translations: Record<LanguageCode, Messages> = {
  ru: {
    localAssistant: "Локальный помощник", permissions: "Доступы", openChat: "Открыть диалог",
    language: "Языки", languageSettings: "Язык и локализация", interfaceLanguage: "Язык интерфейса",
    commandLanguage: "Язык команд и ответов", languageHint: "Настройки сохраняются на этом компьютере.", done: "Готово",
    voiceMode: "ГОЛОСОВОЙ РЕЖИМ", idle: "Готов к голосовой команде", listening: "Слушаю вас",
    thinking: "Обрабатываю команду", speaking: "Отвечаю голосом", paused: "Микрофон на паузе",
    error: "Голосовой режим недоступен", sayCommand: "Скажите команду…", pauseMicrophone: "Поставить на паузу",
    enableMicrophone: "Включить микрофон", quickStart: "БЫСТРЫЙ СТАРТ", whatCanSay: "Что можно сказать",
    localOnly: "Без передачи файлов", systemCommand: "Покажи информацию о системе",
    systemDescription: "ОС, архитектура и оперативная память", chatCommand: "Открой диалог",
    chatDescription: "Показать текстовую историю справа", testVoice: "Проверить голос",
    voiceDescription: "каждое нажатие — новая ситуация", history: "ИСТОРИЯ", dialogTitle: "Диалог с C-3PO",
    enterCommand: "Введите команду…", close: "Закрыть", greeting: "Здравствуйте! Чем могу помочь?",
    consultingR2: "Минутку, я советуюсь с R2-D2…",
  },
  en: {
    localAssistant: "Local assistant", permissions: "Permissions", openChat: "Open chat", language: "Languages",
    languageSettings: "Language and localization", interfaceLanguage: "Interface language",
    commandLanguage: "Commands and replies", languageHint: "These settings are stored on this computer.", done: "Done",
    voiceMode: "VOICE MODE", idle: "Ready for a voice command", listening: "Listening",
    thinking: "Processing your command", speaking: "Speaking", paused: "Microphone paused",
    error: "Voice mode is unavailable", sayCommand: "Say a command…", pauseMicrophone: "Pause microphone",
    enableMicrophone: "Enable microphone", quickStart: "QUICK START", whatCanSay: "Things you can say",
    localOnly: "No file transfer", systemCommand: "Show system information",
    systemDescription: "OS, architecture, and memory", chatCommand: "Open chat",
    chatDescription: "Show text history on the right", testVoice: "Test voice",
    voiceDescription: "each click uses a new situation", history: "HISTORY", dialogTitle: "Chat with C-3PO",
    enterCommand: "Enter a command…", close: "Close", greeting: "Hello! How may I help?",
    consultingR2: "One moment, I am consulting R2-D2…",
  },
  es: {
    localAssistant: "Asistente local", permissions: "Permisos", openChat: "Abrir chat", language: "Idiomas",
    languageSettings: "Idioma y localización", interfaceLanguage: "Idioma de la interfaz",
    commandLanguage: "Comandos y respuestas", languageHint: "La configuración se guarda en este equipo.", done: "Listo",
    voiceMode: "MODO DE VOZ", idle: "Listo para un comando de voz", listening: "Te escucho",
    thinking: "Procesando el comando", speaking: "Respondiendo", paused: "Micrófono en pausa",
    error: "El modo de voz no está disponible", sayCommand: "Di un comando…", pauseMicrophone: "Pausar micrófono",
    enableMicrophone: "Activar micrófono", quickStart: "INICIO RÁPIDO", whatCanSay: "Qué puedes decir",
    localOnly: "Sin transferir archivos", systemCommand: "Muestra información del sistema",
    systemDescription: "SO, arquitectura y memoria", chatCommand: "Abrir chat", chatDescription: "Mostrar el historial a la derecha",
    testVoice: "Probar voz", voiceDescription: "cada clic usa una situación nueva", history: "HISTORIAL",
    dialogTitle: "Chat con C-3PO", enterCommand: "Escribe un comando…", close: "Cerrar",
    greeting: "¡Hola! ¿En qué puedo ayudarte?", consultingR2: "Un momento, estoy consultando a R2-D2…",
  },
  de: {
    localAssistant: "Lokaler Assistent", permissions: "Berechtigungen", openChat: "Chat öffnen", language: "Sprachen",
    languageSettings: "Sprache und Lokalisierung", interfaceLanguage: "Sprache der Oberfläche",
    commandLanguage: "Befehle und Antworten", languageHint: "Die Einstellungen werden auf diesem Computer gespeichert.", done: "Fertig",
    voiceMode: "SPRACHMODUS", idle: "Bereit für einen Sprachbefehl", listening: "Ich höre zu",
    thinking: "Befehl wird verarbeitet", speaking: "Ich antworte", paused: "Mikrofon pausiert",
    error: "Sprachmodus ist nicht verfügbar", sayCommand: "Sagen Sie einen Befehl…", pauseMicrophone: "Mikrofon pausieren",
    enableMicrophone: "Mikrofon aktivieren", quickStart: "SCHNELLSTART", whatCanSay: "Mögliche Befehle",
    localOnly: "Keine Dateiübertragung", systemCommand: "Systeminformationen anzeigen",
    systemDescription: "Betriebssystem, Architektur und Speicher", chatCommand: "Chat öffnen",
    chatDescription: "Textverlauf rechts anzeigen", testVoice: "Stimme testen", voiceDescription: "jeder Klick nutzt eine neue Situation",
    history: "VERLAUF", dialogTitle: "Chat mit C-3PO", enterCommand: "Befehl eingeben…", close: "Schließen",
    greeting: "Hallo! Wie kann ich helfen?", consultingR2: "Einen Moment, ich frage R2-D2…",
  },
  fr: {
    localAssistant: "Assistant local", permissions: "Autorisations", openChat: "Ouvrir le dialogue", language: "Langues",
    languageSettings: "Langue et localisation", interfaceLanguage: "Langue de l’interface",
    commandLanguage: "Commandes et réponses", languageHint: "Ces réglages sont conservés sur cet ordinateur.", done: "Terminé",
    voiceMode: "MODE VOCAL", idle: "Prêt pour une commande vocale", listening: "Je vous écoute",
    thinking: "Traitement de la commande", speaking: "Je réponds", paused: "Microphone en pause",
    error: "Le mode vocal est indisponible", sayCommand: "Dites une commande…", pauseMicrophone: "Mettre en pause",
    enableMicrophone: "Activer le microphone", quickStart: "DÉMARRAGE RAPIDE", whatCanSay: "Ce que vous pouvez dire",
    localOnly: "Aucun transfert de fichiers", systemCommand: "Afficher les informations système",
    systemDescription: "Système, architecture et mémoire", chatCommand: "Ouvrir le dialogue",
    chatDescription: "Afficher l’historique à droite", testVoice: "Tester la voix", voiceDescription: "chaque clic utilise une nouvelle situation",
    history: "HISTORIQUE", dialogTitle: "Dialogue avec C-3PO", enterCommand: "Saisissez une commande…", close: "Fermer",
    greeting: "Bonjour ! Comment puis-je vous aider ?", consultingR2: "Un instant, je consulte R2-D2…",
  },
  it: {
    localAssistant: "Assistente locale", permissions: "Permessi", openChat: "Apri chat", language: "Lingue",
    languageSettings: "Lingua e localizzazione", interfaceLanguage: "Lingua dell’interfaccia",
    commandLanguage: "Comandi e risposte", languageHint: "Le impostazioni vengono salvate su questo computer.", done: "Fatto",
    voiceMode: "MODALITÀ VOCALE", idle: "Pronto per un comando vocale", listening: "Ti ascolto",
    thinking: "Elaborazione del comando", speaking: "Sto rispondendo", paused: "Microfono in pausa",
    error: "Modalità vocale non disponibile", sayCommand: "Pronuncia un comando…", pauseMicrophone: "Metti in pausa",
    enableMicrophone: "Attiva microfono", quickStart: "AVVIO RAPIDO", whatCanSay: "Cosa puoi dire",
    localOnly: "Nessun trasferimento di file", systemCommand: "Mostra informazioni di sistema",
    systemDescription: "Sistema, architettura e memoria", chatCommand: "Apri chat", chatDescription: "Mostra la cronologia a destra",
    testVoice: "Prova la voce", voiceDescription: "ogni clic usa una nuova situazione", history: "CRONOLOGIA",
    dialogTitle: "Chat con C-3PO", enterCommand: "Inserisci un comando…", close: "Chiudi",
    greeting: "Salve! Come posso aiutarti?", consultingR2: "Un momento, consulto R2-D2…",
  },
  pt: {
    localAssistant: "Assistente local", permissions: "Permissões", openChat: "Abrir conversa", language: "Idiomas",
    languageSettings: "Idioma e localização", interfaceLanguage: "Idioma da interface",
    commandLanguage: "Comandos e respostas", languageHint: "As configurações são salvas neste computador.", done: "Concluído",
    voiceMode: "MODO DE VOZ", idle: "Pronto para um comando de voz", listening: "Estou ouvindo",
    thinking: "Processando o comando", speaking: "Respondendo", paused: "Microfone pausado",
    error: "Modo de voz indisponível", sayCommand: "Diga um comando…", pauseMicrophone: "Pausar microfone",
    enableMicrophone: "Ativar microfone", quickStart: "INÍCIO RÁPIDO", whatCanSay: "O que você pode dizer",
    localOnly: "Sem transferência de arquivos", systemCommand: "Mostrar informações do sistema",
    systemDescription: "Sistema, arquitetura e memória", chatCommand: "Abrir conversa", chatDescription: "Mostrar histórico à direita",
    testVoice: "Testar voz", voiceDescription: "cada clique usa uma nova situação", history: "HISTÓRICO",
    dialogTitle: "Conversa com C-3PO", enterCommand: "Digite um comando…", close: "Fechar",
    greeting: "Olá! Como posso ajudar?", consultingR2: "Um momento, estou consultando R2-D2…",
  },
  zh: {
    localAssistant: "本地助手", permissions: "权限", openChat: "打开对话", language: "语言",
    languageSettings: "语言和本地化", interfaceLanguage: "界面语言", commandLanguage: "命令和回答语言",
    languageHint: "设置保存在此电脑上。", done: "完成", voiceMode: "语音模式", idle: "已准备接收语音命令",
    listening: "正在聆听", thinking: "正在处理命令", speaking: "正在回答", paused: "麦克风已暂停",
    error: "语音模式不可用", sayCommand: "请说出命令…", pauseMicrophone: "暂停麦克风", enableMicrophone: "启用麦克风",
    quickStart: "快速开始", whatCanSay: "可以这样说", localOnly: "不传输文件", systemCommand: "显示系统信息",
    systemDescription: "操作系统、架构和内存", chatCommand: "打开对话", chatDescription: "在右侧显示文字记录",
    testVoice: "测试语音", voiceDescription: "每次点击使用一个新场景", history: "历史", dialogTitle: "与 C-3PO 对话",
    enterCommand: "输入命令…", close: "关闭", greeting: "您好！我能为您做些什么？", consultingR2: "请稍候，我正在咨询 R2-D2…",
  },
  ja: {
    localAssistant: "ローカルアシスタント", permissions: "権限", openChat: "チャットを開く", language: "言語",
    languageSettings: "言語とローカライズ", interfaceLanguage: "表示言語", commandLanguage: "コマンドと回答の言語",
    languageHint: "設定はこのコンピューターに保存されます。", done: "完了", voiceMode: "音声モード",
    idle: "音声コマンドを待っています", listening: "聞いています", thinking: "コマンドを処理中",
    speaking: "回答中", paused: "マイクは一時停止中", error: "音声モードを利用できません",
    sayCommand: "コマンドを話してください…", pauseMicrophone: "マイクを一時停止", enableMicrophone: "マイクを有効化",
    quickStart: "クイックスタート", whatCanSay: "使用できるコマンド", localOnly: "ファイル送信なし",
    systemCommand: "システム情報を表示", systemDescription: "OS、アーキテクチャ、メモリ",
    chatCommand: "チャットを開く", chatDescription: "右側に履歴を表示", testVoice: "音声をテスト",
    voiceDescription: "クリックごとに新しい場面", history: "履歴", dialogTitle: "C-3PO とのチャット",
    enterCommand: "コマンドを入力…", close: "閉じる", greeting: "こんにちは！何をお手伝いしましょうか？",
    consultingR2: "少々お待ちください。R2-D2に相談しています…",
  },
  ar: {
    localAssistant: "مساعد محلي", permissions: "الأذونات", openChat: "فتح المحادثة", language: "اللغات",
    languageSettings: "اللغة والتوطين", interfaceLanguage: "لغة الواجهة", commandLanguage: "لغة الأوامر والردود",
    languageHint: "تُحفظ الإعدادات على هذا الكمبيوتر.", done: "تم", voiceMode: "الوضع الصوتي",
    idle: "جاهز لأمر صوتي", listening: "أستمع إليك", thinking: "جارٍ معالجة الأمر", speaking: "جارٍ الرد",
    paused: "الميكروفون متوقف مؤقتًا", error: "الوضع الصوتي غير متاح", sayCommand: "قل أمرًا…",
    pauseMicrophone: "إيقاف الميكروفون مؤقتًا", enableMicrophone: "تشغيل الميكروفون", quickStart: "بدء سريع",
    whatCanSay: "ما يمكنك قوله", localOnly: "دون نقل الملفات", systemCommand: "اعرض معلومات النظام",
    systemDescription: "النظام والبنية والذاكرة", chatCommand: "افتح المحادثة", chatDescription: "عرض السجل النصي على اليمين",
    testVoice: "اختبار الصوت", voiceDescription: "كل نقرة تستخدم موقفًا جديدًا", history: "السجل",
    dialogTitle: "محادثة مع C-3PO", enterCommand: "أدخل أمرًا…", close: "إغلاق",
    greeting: "مرحبًا! كيف يمكنني مساعدتك؟", consultingR2: "لحظة من فضلك، أستشير R2-D2…",
  },
};

export function isLanguageCode(value: string | null): value is LanguageCode {
  return languages.some((language) => language.code === value);
}
