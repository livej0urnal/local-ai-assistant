<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import droidPortrait from "./assets/protocol-droid.png";
import { assistantPhrase, moviePhrase, voiceDemoPhrases } from "./assistantPhrases";
import { isLanguageCode, languages, translations, type LanguageCode } from "./i18n";

type Message = {
  id: number;
  role: "assistant" | "user";
  text: string;
};

type UserProfile = {
  id: string;
  name: string;
  createdAt: string;
};

type ConversationThread = {
  id: string;
  title: string;
  updatedAt: string;
  messages: Message[];
};

type SystemInfo = {
  operatingSystem: string;
  osVersion: string;
  hostName: string;
  architecture: string;
  totalMemoryMb: number;
};

type ProcessMetrics = {
  pid: number;
  residentMemoryBytes: number;
  virtualMemoryBytes: number;
  capturedAtUnixMs: number;
};

type LocalModelFile = {
  name: string;
  path: string;
  sizeBytes: number;
  validGguf: boolean;
};

type LlmStatus = {
  state: "engine-missing" | "model-missing" | "model-invalid" | "ready";
  engineFound: boolean;
  enginePath: string | null;
  modelDirectory: string;
  selectedModel: LocalModelFile | null;
  discoveredModels: number;
  message: string;
};

type LlmRuntimeStatus = {
  state: "stopped" | "loading" | "ready";
  ready: boolean;
  processId: number | null;
  residentMemoryBytes: number;
  address: string;
  message: string;
};

type LocalReply = {
  text: string;
};

type NativeCapability = {
  id: string;
  available: boolean;
  permissionRequired: boolean;
  state: string;
};

type EnvironmentReport = {
  system: SystemInfo;
  platform: string;
  family: string;
  isAppBundle: boolean;
  capabilities: NativeCapability[];
};

type PermissionState = "checking" | "prompt" | "granted" | "denied" | "unsupported";
type VoiceState = "idle" | "listening" | "thinking" | "speaking" | "paused" | "error";

type AppError = {
  title: string;
  message: string;
  guidance: string;
  timestamp: string;
};

type RecognitionResultItem = {
  isFinal: boolean;
  0: { transcript: string };
};

type RecognitionResultList = {
  length: number;
  [index: number]: RecognitionResultItem;
};

type RecognitionLike = {
  lang: string;
  continuous: boolean;
  interimResults: boolean;
  onstart: (() => void) | null;
  onend: (() => void) | null;
  onerror: ((event: { error: string }) => void) | null;
  onresult: ((event: { resultIndex: number; results: RecognitionResultList }) => void) | null;
  start: () => void;
  stop: () => void;
};

type RecognitionConstructor = new () => RecognitionLike;

const storedInterfaceLanguage = window.localStorage.getItem("c3po.interface-language");
const storedCommandLanguage = window.localStorage.getItem("c3po.command-language");
const interfaceLanguage = ref<LanguageCode>(isLanguageCode(storedInterfaceLanguage) ? storedInterfaceLanguage : "ru");
const commandLanguage = ref<LanguageCode>(isLanguageCode(storedCommandLanguage) ? storedCommandLanguage : "ru");

function readProfiles(): UserProfile[] {
  try {
    const value = JSON.parse(window.localStorage.getItem("c3po.user-profiles") ?? "[]");
    return Array.isArray(value) ? value.filter((item) => item?.id && looksLikePersonName(item?.name)) : [];
  } catch {
    return [];
  }
}

const profiles = ref<UserProfile[]>(readProfiles());
// Перезаписываем список после проверки, чтобы удалить ранее ошибочно
// сохранённые ответы вроде «хочу» или «да».
window.localStorage.setItem("c3po.user-profiles", JSON.stringify(profiles.value));
const storedProfileId = window.localStorage.getItem("c3po.current-profile");
const initialProfile = profiles.value.find((profile) => profile.id === storedProfileId) ?? profiles.value[0] ?? null;
const currentProfileId = ref(initialProfile?.id ?? "");
const awaitingIntroduction = ref(!initialProfile);

function t(key: keyof (typeof translations)["ru"]) {
  return translations[interfaceLanguage.value][key];
}

function commandText(key: keyof (typeof translations)["ru"]) {
  return translations[commandLanguage.value][key];
}

const profilePhrases: Record<LanguageCode, { intro: string; met: string; newUser: string }> = {
  ru: { intro: "Давайте познакомимся. Как вас зовут?", met: "Очень приятно познакомиться", newUser: "Я слышу нового собеседника. Пожалуйста, представьтесь." },
  en: { intro: "Let us get acquainted. What is your name?", met: "It is a pleasure to meet you", newUser: "I hear a new speaker. Please introduce yourself." },
  es: { intro: "Conozcámonos. ¿Cómo te llamas?", met: "Encantado de conocerte", newUser: "Escucho a un nuevo usuario. Por favor, preséntate." },
  de: { intro: "Lernen wir uns kennen. Wie heißen Sie?", met: "Sehr erfreut", newUser: "Ich höre eine neue Person. Bitte stellen Sie sich vor." },
  fr: { intro: "Faisons connaissance. Comment vous appelez-vous ?", met: "Enchanté de vous rencontrer", newUser: "J’entends une nouvelle personne. Veuillez vous présenter." },
  it: { intro: "Conosciamoci. Come ti chiami?", met: "Piacere di conoscerti", newUser: "Sento una nuova persona. Per favore, presentati." },
  pt: { intro: "Vamos nos conhecer. Como você se chama?", met: "Prazer em conhecer você", newUser: "Ouço uma nova pessoa. Por favor, apresente-se." },
  zh: { intro: "让我们认识一下。您叫什么名字？", met: "很高兴认识您", newUser: "我听到了一位新用户。请介绍一下自己。" },
  ja: { intro: "はじめまして。お名前を教えてください。", met: "お会いできて光栄です", newUser: "新しい方ですね。お名前を教えてください。" },
  ar: { intro: "لنتعارف. ما اسمك؟", met: "سعيد بلقائك", newUser: "أسمع متحدثًا جديدًا. يرجى التعريف بنفسك." },
};

const profileLabels: Record<LanguageCode, { current: string; newUser: string; localMemory: string }> = {
  ru: { current: "Текущий собеседник", newUser: "Познакомиться с новым человеком", localMemory: "Имена и история хранятся только на этом компьютере." },
  en: { current: "Current speaker", newUser: "Meet a new person", localMemory: "Names and history stay on this computer." },
  es: { current: "Usuario actual", newUser: "Conocer a otra persona", localMemory: "Los nombres y el historial permanecen en este equipo." },
  de: { current: "Aktuelle Person", newUser: "Neue Person kennenlernen", localMemory: "Namen und Verlauf bleiben auf diesem Computer." },
  fr: { current: "Interlocuteur actuel", newUser: "Rencontrer une autre personne", localMemory: "Les noms et l’historique restent sur cet ordinateur." },
  it: { current: "Persona attuale", newUser: "Conosci una nuova persona", localMemory: "Nomi e cronologia restano su questo computer." },
  pt: { current: "Pessoa atual", newUser: "Conhecer uma nova pessoa", localMemory: "Nomes e histórico ficam neste computador." },
  zh: { current: "当前用户", newUser: "认识新用户", localMemory: "姓名和历史记录仅保存在此电脑上。" },
  ja: { current: "現在のユーザー", newUser: "新しい人と話す", localMemory: "名前と履歴はこのコンピューターだけに保存されます。" },
  ar: { current: "المتحدث الحالي", newUser: "التعرف على شخص جديد", localMemory: "تبقى الأسماء والسجل على هذا الكمبيوتر فقط." },
};

const dialogLabels: Record<LanguageCode, { list: string; newDialog: string; empty: string }> = {
  ru: { list: "История диалогов", newDialog: "Новый диалог", empty: "Диалогов пока нет" },
  en: { list: "Conversation history", newDialog: "New conversation", empty: "No conversations yet" },
  es: { list: "Historial de conversaciones", newDialog: "Nueva conversación", empty: "Aún no hay conversaciones" },
  de: { list: "Gesprächsverlauf", newDialog: "Neues Gespräch", empty: "Noch keine Gespräche" },
  fr: { list: "Historique des dialogues", newDialog: "Nouveau dialogue", empty: "Aucun dialogue" },
  it: { list: "Cronologia dialoghi", newDialog: "Nuovo dialogo", empty: "Nessun dialogo" },
  pt: { list: "Histórico de conversas", newDialog: "Nova conversa", empty: "Nenhuma conversa" },
  zh: { list: "对话历史", newDialog: "新对话", empty: "暂无对话" },
  ja: { list: "会話履歴", newDialog: "新しい会話", empty: "会話はまだありません" },
  ar: { list: "سجل المحادثات", newDialog: "محادثة جديدة", empty: "لا توجد محادثات بعد" },
};

function introductionPrompt(language: LanguageCode) {
  return profilePhrases[language].intro;
}

function readConversationThreads(profileId: string): ConversationThread[] {
  try {
    const value = JSON.parse(window.localStorage.getItem(`c3po.conversations.${profileId}`) ?? "[]");
    if (Array.isArray(value) && value.length) return value as ConversationThread[];

    // Однократная миграция истории из первой версии, где у профиля был один чат.
    const legacy = JSON.parse(window.localStorage.getItem(`c3po.conversation.${profileId}`) ?? "[]") as Message[];
    if (!Array.isArray(legacy) || !legacy.length) return [];
    return [{
      id: crypto.randomUUID(),
      title: legacy.find((message) => message.role === "user")?.text.slice(0, 42) || t("dialogTitle"),
      updatedAt: new Date().toISOString(),
      messages: legacy,
    }];
  } catch {
    return [];
  }
}

function saveProfiles() {
  window.localStorage.setItem("c3po.user-profiles", JSON.stringify(profiles.value));
  if (currentProfileId.value) window.localStorage.setItem("c3po.current-profile", currentProfileId.value);
  else window.localStorage.removeItem("c3po.current-profile");
}

function saveConversation() {
  if (!currentProfileId.value) return;
  let thread = conversationThreads.value.find((item) => item.id === activeConversationId.value);
  if (!thread) {
    thread = {
      id: crypto.randomUUID(),
      title: t("dialogTitle"),
      updatedAt: new Date().toISOString(),
      messages: [],
    };
    activeConversationId.value = thread.id;
    conversationThreads.value.unshift(thread);
  }
  const firstUserMessage = messages.value.find((message) => message.role === "user");
  thread.title = firstUserMessage?.text.trim().slice(0, 42) || t("dialogTitle");
  thread.updatedAt = new Date().toISOString();
  thread.messages = messages.value.slice(-100);
  conversationThreads.value.sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
  window.localStorage.setItem(
    `c3po.conversations.${currentProfileId.value}`,
    JSON.stringify(conversationThreads.value),
  );
}

function extractIntroducedName(text: string) {
  return text
    .replace(/^(меня зовут|я|my name is|i am|i'm|me llamo|soy|ich heiße|ich bin|je m'appelle|je suis|mi chiamo|sono|meu nome é|eu sou|我叫|私は|اسمي)\s+/i, "")
    .replace(/[.!?。！？]+$/u, "")
    .trim()
    .slice(0, 40);
}

function looksLikePersonName(value: unknown): value is string {
  if (typeof value !== "string") return false;
  const name = value.trim().toLocaleLowerCase();
  const nonNames = new Set([
    "хочу", "да", "нет", "привет", "здравствуйте", "хорошо", "конечно", "готов",
    "yes", "no", "hello", "hi", "okay", "ready", "i want",
    "sí", "no", "hola", "ja", "nein", "hallo", "oui", "bonjour",
    "ciao", "sim", "olá", "你好", "はい", "いいえ", "نعم", "لا", "مرحبا",
  ]);
  const wordCount = name.split(/\s+/u).length;
  return name.length <= 40
    && wordCount <= 3
    && !nonNames.has(name)
    && /^[\p{L}\p{M}][\p{L}\p{M}'’ -]*$/u.test(name);
}

function rememberIntroducedUser(text: string) {
  const name = extractIntroducedName(text);
  if (!looksLikePersonName(name)) {
    const response = introductionPrompt(commandLanguage.value);
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
    return false;
  }
  const existing = profiles.value.find((profile) => profile.name.toLocaleLowerCase() === name.toLocaleLowerCase());
  const profile = existing ?? { id: crypto.randomUUID(), name, createdAt: new Date().toISOString() };
  if (!existing) profiles.value.push(profile);
  currentProfileId.value = profile.id;
  awaitingIntroduction.value = false;
  conversationThreads.value = [];
  activeConversationId.value = "";
  messages.value = [];
  nextMessageId.value = 1;
  saveProfiles();
  const response = `${profilePhrases[commandLanguage.value].met}, ${profile.name}.`;
  addMessage("assistant", response);
  lastCommand.value = response;
  languageOpen.value = false;
  chatOpen.value = true;
  speak(response);
  return true;
}

function startNewSpeaker() {
  currentProfileId.value = "";
  awaitingIntroduction.value = true;
  conversationThreads.value = [];
  activeConversationId.value = "";
  saveProfiles();
  messages.value = [];
  nextMessageId.value = 1;
  const response = profilePhrases[commandLanguage.value].newUser;
  addMessage("assistant", response);
  lastCommand.value = response;
  languageOpen.value = false;
  chatOpen.value = true;
  speak(response);
}

function switchProfile() {
  const profile = currentProfile.value;
  if (!profile) {
    startNewSpeaker();
    return;
  }
  awaitingIntroduction.value = false;
  saveProfiles();
  conversationThreads.value = readConversationThreads(profile.id);
  const recentThread = conversationThreads.value[0];
  activeConversationId.value = recentThread?.id ?? "";
  messages.value = recentThread?.messages.length
    ? recentThread.messages
    : [{ id: 1, role: "assistant", text: `${t("greeting")} ${profile.name}.` }];
  nextMessageId.value = Math.max(1, ...messages.value.map((message) => message.id)) + 1;
  conversationListOpen.value = false;
}

function startNewConversation() {
  if (!currentProfile.value) {
    startNewSpeaker();
    return;
  }
  activeConversationId.value = crypto.randomUUID();
  messages.value = [{ id: 1, role: "assistant", text: `${t("greeting")} ${currentProfile.value.name}.` }];
  nextMessageId.value = 2;
  conversationListOpen.value = false;
  saveConversation();
}

function openConversation(thread: ConversationThread) {
  activeConversationId.value = thread.id;
  messages.value = thread.messages;
  nextMessageId.value = Math.max(1, ...messages.value.map((message) => message.id)) + 1;
  conversationListOpen.value = false;
  void scrollChat();
}

function formatConversationDate(value: string) {
  return new Intl.DateTimeFormat(interfaceLanguage.value, {
    day: "2-digit",
    month: "short",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

const initialConversationThreads = initialProfile ? readConversationThreads(initialProfile.id) : [];
const initialConversation = initialConversationThreads[0] ?? null;
const conversationThreads = ref<ConversationThread[]>(initialConversationThreads);
const activeConversationId = ref(initialConversation?.id ?? "");
const conversationListOpen = ref(false);
const messages = ref<Message[]>(initialConversation?.messages.length ? initialConversation.messages : [
  {
    id: 1,
    role: "assistant",
    text: initialProfile ? `${t("greeting")} ${initialProfile.name}.` : introductionPrompt(commandLanguage.value),
  },
]);
const nextMessageId = ref(Math.max(1, ...messages.value.map((message) => message.id)) + 1);
const input = ref("");
const liveTranscript = ref("");
const lastCommand = ref(initialProfile ? t("sayCommand") : introductionPrompt(commandLanguage.value));
const pendingAction = ref(false);
const isWorking = ref(false);
const chatOpen = ref(false);
const permissionsOpen = ref(false);
const llmOpen = ref(false);
const languageOpen = ref(false);
const onboardingOpen = ref(false);
const voiceState = ref<VoiceState>("idle");
const microphonePermission = ref<PermissionState>("checking");
const microphoneDevices = ref(0);
const microphoneActive = ref(false);
const recognitionAvailable = ref(false);
const synthesisAvailable = ref(false);
const audioLevel = ref(0);
const environment = ref<EnvironmentReport | null>(null);
const conversation = ref<HTMLElement | null>(null);
const lastError = ref<AppError | null>(null);
const recentLogs = ref<string[]>([]);
const logPath = ref("");
const logsLoading = ref(false);
const logsExpanded = ref(false);
const processMetrics = ref<ProcessMetrics | null>(null);
const memoryBaselineBytes = ref(0);
const memoryPeakBytes = ref(0);
const selectedVoiceName = ref("System voice");
const recognitionBlockedReason = ref("");
const llmStatus = ref<LlmStatus | null>(null);
const llmLoading = ref(false);
const llmRuntime = ref<LlmRuntimeStatus | null>(null);
const llmTestReply = ref("");

let recognition: RecognitionLike | null = null;
let microphoneStream: MediaStream | null = null;
let audioContext: AudioContext | null = null;
let droidAudioContext: AudioContext | null = null;
let meterFrame = 0;
let restartTimer = 0;
let shouldListen = false;
let memoryTimer = 0;
let voiceDemoIndex = 0;
const microphonePermissionStorageKey = "c3po.microphone-permission";

function savedMicrophonePermission(): PermissionState | null {
  const value = window.localStorage.getItem(microphonePermissionStorageKey);
  return value === "granted" || value === "denied" || value === "prompt" ? value : null;
}

function rememberMicrophonePermission(state: PermissionState) {
  microphonePermission.value = state;
  if (state === "granted" || state === "denied" || state === "prompt") {
    window.localStorage.setItem(microphonePermissionStorageKey, state);
  }
  if ("__TAURI_INTERNALS__" in window && (state === "granted" || state === "denied")) {
    void invoke("set_saved_microphone_approval", { approved: state === "granted" }).catch((error) => {
      console.warn("Не удалось сохранить нативное состояние микрофона", error);
    });
  }
}

async function hasRememberedMicrophoneApproval() {
  const localApproval = savedMicrophonePermission() === "granted";
  if (!("__TAURI_INTERNALS__" in window)) return localApproval;
  try {
    return (await invoke<boolean>("get_saved_microphone_approval")) || localApproval;
  } catch (error) {
    await writeLog("warn", "microphone.permission-memory", `Не удалось прочитать нативную настройку: ${String(error)}`);
    return localApproval;
  }
}

const canSend = computed(() => input.value.trim().length > 0 && !isWorking.value);
const currentProfile = computed(() => profiles.value.find((profile) => profile.id === currentProfileId.value) ?? null);
const selectedCommandLanguage = computed(
  () => languages.find((language) => language.code === commandLanguage.value) ?? languages[0],
);
const systemLabel = computed(() => {
  if (!environment.value) return "Определяю систему…";
  return `${environment.value.system.operatingSystem} ${environment.value.system.osVersion}`;
});
const voiceStatus = computed(() => {
  if (voiceState.value === "idle" && microphoneActive.value && !recognitionAvailable.value) {
    return t("idle");
  }
  const labels: Record<VoiceState, keyof (typeof translations)["ru"]> = {
    idle: "idle",
    listening: "listening",
    thinking: "thinking",
    speaking: "speaking",
    paused: "paused",
    error: "error",
  };
  return t(labels[voiceState.value]);
});
const microphoneLabel = computed(() => {
  const labels: Record<PermissionState, string> = {
    checking: "Проверка…",
    prompt: "Нужно разрешение",
    granted: microphoneDevices.value > 0 ? `Разрешён · устройств: ${microphoneDevices.value}` : "Разрешён",
    denied: "Запрещён системой",
    unsupported: "Не поддерживается",
  };
  return labels[microphonePermission.value];
});
const canPauseMicrophone = computed(() => microphoneActive.value || voiceState.value === "speaking");
const processMemoryLabel = computed(() => formatBytes(processMetrics.value?.residentMemoryBytes ?? 0));
const processMemoryDelta = computed(() => {
  if (!processMetrics.value || !memoryBaselineBytes.value) return "—";
  const delta = processMetrics.value.residentMemoryBytes - memoryBaselineBytes.value;
  const sign = delta > 0 ? "+" : "";
  return `${sign}${formatBytes(delta)}`;
});
const logSummary = computed(() => {
  const errors = recentLogs.value.filter((line) => line.includes("| ERROR |")).length;
  const warnings = recentLogs.value.filter((line) => line.includes("| WARN ")).length;
  if (!recentLogs.value.length) return "Записей пока нет";
  if (!errors && !warnings) return `${recentLogs.value.length} событий · ошибок нет`;
  return `${recentLogs.value.length} событий · ошибок ${errors} · предупреждений ${warnings}`;
});
const llmShortLabel = computed(() => {
  if (!llmStatus.value) return "Проверяю модель";
  const labels: Record<LlmStatus["state"], string> = {
    "engine-missing": "Нужен llama.cpp",
    "model-missing": "Нужна GGUF-модель",
    "model-invalid": "Ошибка модели",
    ready: "R2-D2 готов",
  };
  return labels[llmStatus.value.state];
});

function formatBytes(bytes: number) {
  const absolute = Math.abs(bytes);
  if (absolute < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} КБ`;
  if (absolute >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} ГБ`;
  return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
}

async function loadLlmStatus() {
  if (!("__TAURI_INTERNALS__" in window)) {
    llmStatus.value = {
      state: "model-missing",
      engineFound: true,
      enginePath: "Браузерный предпросмотр",
      modelDirectory: "Доступно только в приложении Tauri",
      selectedModel: null,
      discoveredModels: 0,
      message: "В браузерном режиме показано демонстрационное состояние локальной модели.",
    };
    return;
  }

  llmLoading.value = true;
  try {
    llmStatus.value = await invoke<LlmStatus>("get_llm_status");
    llmRuntime.value = await invoke<LlmRuntimeStatus>("get_llm_runtime_status");
  } catch (error) {
    const details = errorDetails(error);
    await writeLog("error", "llm.status", `${details.name}: ${details.message}`);
  } finally {
    llmLoading.value = false;
  }
}

async function waitForLocalModel() {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    const status = await invoke<LlmRuntimeStatus>("get_llm_runtime_status");
    llmRuntime.value = status;
    if (status.ready) return true;
    if (status.state === "stopped" && attempt > 0) return false;
    await new Promise((resolve) => window.setTimeout(resolve, 500));
  }
  return false;
}

async function startLocalModel() {
  if (!("__TAURI_INTERNALS__" in window) || llmStatus.value?.state !== "ready") return false;
  llmLoading.value = true;
  llmTestReply.value = "";
  try {
    llmRuntime.value = await invoke<LlmRuntimeStatus>("start_llm");
    const ready = await waitForLocalModel();
    if (!ready) throw new Error("llama-server не успел загрузить модель");
    await refreshProcessMetrics();
    return true;
  } catch (error) {
    const details = errorDetails(error);
    llmTestReply.value = `${details.name}: ${details.message}`;
    await writeLog("error", "llm.start", llmTestReply.value);
    return false;
  } finally {
    llmLoading.value = false;
  }
}

async function stopLocalModel() {
  if (!("__TAURI_INTERNALS__" in window)) return;
  llmRuntime.value = await invoke<LlmRuntimeStatus>("stop_llm");
  llmTestReply.value = moviePhrase("shutdown");
  void playR2D2Signal("shutdown");
  await refreshProcessMetrics();
}

type R2D2Mood = "thinking" | "success" | "error" | "shutdown";

async function playR2D2Signal(mood: R2D2Mood) {
  // Это оригинальные процедурно созданные сигналы, а не аудиофрагменты фильма.
  // Позже этот слой можно заменить лицензированным звуковым пакетом.
  droidAudioContext ??= new AudioContext();
  if (droidAudioContext.state === "suspended") await droidAudioContext.resume();

  const patterns: Record<R2D2Mood, Array<[number, number, number]>> = {
    thinking: [[720, 0.07, 980], [1040, 0.06, 820], [620, 0.09, 1320], [1180, 0.05, 900]],
    success: [[680, 0.06, 940], [940, 0.07, 1320], [1240, 0.1, 1580]],
    error: [[940, 0.08, 520], [560, 0.1, 340], [760, 0.07, 420]],
    shutdown: [[1100, 0.07, 760], [720, 0.09, 480], [440, 0.14, 220]],
  };

  let cursor = droidAudioContext.currentTime + 0.02;
  for (const [startFrequency, duration, endFrequency] of patterns[mood]) {
    const oscillator = droidAudioContext.createOscillator();
    const gain = droidAudioContext.createGain();
    oscillator.type = "sine";
    oscillator.frequency.setValueAtTime(startFrequency, cursor);
    oscillator.frequency.exponentialRampToValueAtTime(endFrequency, cursor + duration);
    gain.gain.setValueAtTime(0.0001, cursor);
    gain.gain.exponentialRampToValueAtTime(0.12, cursor + 0.012);
    gain.gain.exponentialRampToValueAtTime(0.0001, cursor + duration);
    oscillator.connect(gain);
    gain.connect(droidAudioContext.destination);
    oscillator.start(cursor);
    oscillator.stop(cursor + duration + 0.01);
    cursor += duration + 0.035;
  }
}

async function askLocalModel(prompt: string, showInConversation = true) {
  isWorking.value = true;
  voiceState.value = "thinking";
  lastCommand.value = t("consultingR2");
  try {
    await playR2D2Signal("thinking");
    if (!llmRuntime.value?.ready && !(await startLocalModel())) {
      throw new Error("Локальная модель не запустилась");
    }
    const reply = await invoke<LocalReply>("generate_local_reply", {
      prompt,
      language: selectedCommandLanguage.value.modelName,
      userName: currentProfile.value?.name ?? null,
    });
    llmTestReply.value = reply.text;
    lastCommand.value = reply.text;
    await playR2D2Signal("success");
    if (showInConversation) addMessage("assistant", reply.text);
    speak(reply.text);
    return reply.text;
  } catch (error) {
    await playR2D2Signal("error");
    const details = errorDetails(error);
    const message = `Локальная модель не ответила: ${details.message}`;
    llmTestReply.value = message;
    if (showInConversation) addMessage("assistant", message);
    voiceState.value = "error";
    await writeLog("error", "llm.generate", `${details.name}: ${details.message}`);
    return "";
  } finally {
    isWorking.value = false;
  }
}

async function testLocalModel() {
  llmTestReply.value = "C-3PO советуется с R2-D2…";
  await askLocalModel("Коротко поприветствуй пользователя от лица C-3PO и объясни, что размышления выполняются локально.", false);
}

async function openLlmPanel() {
  llmOpen.value = true;
  await loadLlmStatus();
}

async function refreshProcessMetrics() {
  if (!("__TAURI_INTERNALS__" in window)) return;

  try {
    const metrics = await invoke<ProcessMetrics>("get_process_metrics");
    processMetrics.value = metrics;
    if (!memoryBaselineBytes.value) memoryBaselineBytes.value = metrics.residentMemoryBytes;
    memoryPeakBytes.value = Math.max(memoryPeakBytes.value, metrics.residentMemoryBytes);
  } catch (error) {
    const details = errorDetails(error);
    await writeLog("warn", "process.metrics", `${details.name}: ${details.message}`);
  }
}

function errorDetails(error: unknown) {
  if (error instanceof DOMException || error instanceof Error) {
    return {
      name: error.name || "Error",
      message: error.message || "Без дополнительного описания",
      stack: error.stack || "",
    };
  }
  return { name: "UnknownError", message: String(error), stack: "" };
}

async function writeLog(
  level: "info" | "warn" | "error",
  source: string,
  message: string,
  details?: Record<string, unknown>,
) {
  // В журнал попадают только диагностические сведения. Звук и распознанный
  // текст здесь намеренно не передаются и не сохраняются.
  const entry = {
    level,
    source,
    message,
    details: details ? JSON.stringify(details) : null,
    timestamp: new Date().toISOString(),
  };

  if (!("__TAURI_INTERNALS__" in window)) {
    console[level](source, message, details ?? "");
    return;
  }

  try {
    await invoke("log_client_event", { entry });
  } catch (logError) {
    console.error("Не удалось записать событие в журнал C-3PO", logError);
  }
}

function microphoneErrorDescription(error: unknown): AppError {
  // Браузерные движки возвращают стандартные имена ошибок. Переводим их
  // в конкретную подсказку, чтобы пользователю не пришлось искать код ошибки.
  const details = errorDetails(error);
  const guidance: Record<string, string> = {
    NotAllowedError:
      "Откройте Системные настройки → Конфиденциальность и безопасность → Микрофон и разрешите доступ для C-3PO. Затем полностью перезапустите приложение.",
    NotFoundError: "Подключите микрофон или выберите доступное устройство ввода в настройках звука.",
    NotReadableError:
      "Закройте приложение, которое могло занять микрофон, и попробуйте снова. Также проверьте выбранный источник звука.",
    AbortError: "Запрос был прерван системой. Попробуйте включить микрофон ещё раз.",
    SecurityError: "Система заблокировала доступ из текущего режима запуска. Попробуйте собранное приложение C-3PO.",
    TypeError: "Окно приложения не поддерживает требуемый способ доступа к микрофону.",
  };

  return {
    title: `Микрофон: ${details.name}`,
    message: details.message,
    guidance: guidance[details.name] ?? "Откройте журнал ниже: в нём сохранены технические подробности ошибки.",
    timestamp: new Date().toLocaleString("ru-RU"),
  };
}

async function refreshLogs() {
  if (!("__TAURI_INTERNALS__" in window)) return;
  logsLoading.value = true;
  try {
    [recentLogs.value, logPath.value] = await Promise.all([
      invoke<string[]>("get_recent_logs", { limit: 40 }),
      invoke<string>("get_log_path"),
    ]);
  } catch (error) {
    console.error("Не удалось прочитать журнал C-3PO", error);
  } finally {
    logsLoading.value = false;
  }
}

async function openPermissions() {
  permissionsOpen.value = true;
  logsExpanded.value = false;
  await refreshLogs();
}

async function scrollChat() {
  await nextTick();
  conversation.value?.scrollTo({ top: conversation.value.scrollHeight, behavior: "smooth" });
}

function addMessage(role: Message["role"], text: string) {
  messages.value.push({ id: nextMessageId.value++, role, text });
  saveConversation();
  void scrollChat();
}

async function loadEnvironment() {
  if ("__TAURI_INTERNALS__" in window) {
    environment.value = await invoke<EnvironmentReport>("get_environment_report");
    return;
  }

  environment.value = {
    platform: navigator.platform.toLowerCase().includes("mac") ? "macos" : "browser",
    family: "browser",
    isAppBundle: false,
    system: {
      operatingSystem: "Режим браузера",
      osVersion: "Предпросмотр",
      hostName: "localhost",
      architecture: navigator.platform || "unknown",
      totalMemoryMb: 0,
    },
    capabilities: [
      { id: "system-info", available: true, permissionRequired: false, state: "available" },
      { id: "microphone", available: Boolean(navigator.mediaDevices), permissionRequired: true, state: "requires-runtime-check" },
      { id: "files", available: true, permissionRequired: true, state: "on-demand" },
      { id: "screen-capture", available: true, permissionRequired: true, state: "not-requested" },
      { id: "accessibility", available: false, permissionRequired: true, state: "not-requested" },
    ],
  };
}

function getRecognitionConstructor(): RecognitionConstructor | undefined {
  const voiceWindow = window as typeof window & {
    SpeechRecognition?: RecognitionConstructor;
    webkitSpeechRecognition?: RecognitionConstructor;
  };
  return voiceWindow.SpeechRecognition ?? voiceWindow.webkitSpeechRecognition;
}

async function handleRecognizedSpeech(text: string) {
  // Системные распознаватели записывают имя по-разному, поэтому принимаем
  // латинское C-3PO и наиболее частые русские транскрипции.
  const wakeWord = /(?:\bc\s*[- ]?3\s*[- ]?p\s*[- ]?o\b|\bc3po\b|си\s*[- ]?три\s*[- ]?пи\s*[- ]?о|ситрипио)/iu;
  const wakeMatch = text.match(wakeWord);

  if (!wakeMatch) {
    // Пока окно скрыто, обычную речь игнорируем: C-3PO просыпается только
    // после своего имени и не превращает фоновые разговоры в команды.
    if (document.visibilityState === "visible") processVoiceCommand(text);
    return;
  }

  if ("__TAURI_INTERNALS__" in window) {
    await invoke("show_main_window");
  }
  const command = text.replace(wakeWord, "").replace(/^[,.:;!?\s-]+/u, "").trim();
  if (command) {
    processVoiceCommand(command);
  } else {
    liveTranscript.value = "";
    lastCommand.value = t("listening");
    voiceState.value = "listening";
  }
}

function configureRecognition() {
  const Recognition = getRecognitionConstructor();
  synthesisAvailable.value = "speechSynthesis" in window;

  // macOS аварийно завершает «голый» tauri dev-процесс при обращении к
  // SFSpeechRecognition. Системный API безопасно включаем только внутри .app,
  // где TCC читает Contents/Info.plist. Сам микрофон в dev-режиме остаётся доступен.
  if (environment.value?.platform === "macos" && !environment.value.isAppBundle) {
    recognitionAvailable.value = false;
    recognitionBlockedReason.value =
      "Распознавание отключено в отладочном файле macOS. Запустите конфигурацию «C-3PO — Приложение», которая открывает пакет .app.";
    void writeLog("warn", "voice.recognition", "Распознавание пропущено вне macOS .app для защиты от TCC-падения");
    return;
  }

  recognitionAvailable.value = Boolean(Recognition);
  recognitionBlockedReason.value = Recognition ? "" : "Системное распознавание речи недоступно в этом WebView.";

  if (!Recognition) return;

  recognition = new Recognition();
  recognition.lang = selectedCommandLanguage.value.speechCode;
  recognition.continuous = true;
  recognition.interimResults = true;
  recognition.onstart = () => {
    voiceState.value = "listening";
  };
  recognition.onend = () => {
    if (shouldListen && voiceState.value !== "speaking") {
      window.clearTimeout(restartTimer);
      restartTimer = window.setTimeout(startRecognition, 350);
    }
  };
  recognition.onerror = (event) => {
    void writeLog("warn", "voice.recognition", `Ошибка распознавания: ${event.error}`, {
      recognitionAvailable: recognitionAvailable.value,
      microphonePermission: microphonePermission.value,
    });
    if (event.error === "not-allowed" || event.error === "service-not-allowed") {
      microphonePermission.value = "denied";
      voiceState.value = "error";
      shouldListen = false;
      return;
    }

    if (event.error !== "no-speech" && event.error !== "aborted") {
      voiceState.value = "error";
    }
  };
  recognition.onresult = (event) => {
    let finalText = "";
    let interimText = "";

    for (let index = event.resultIndex; index < event.results.length; index += 1) {
      const result = event.results[index];
      if (result.isFinal) finalText += result[0].transcript;
      else interimText += result[0].transcript;
    }

    liveTranscript.value = interimText || finalText;
    if (finalText.trim()) void handleRecognizedSpeech(finalText.trim());
  };
}

function startRecognition() {
  if (!recognition || microphonePermission.value !== "granted") return;

  shouldListen = true;
  try {
    recognition.start();
  } catch {
    // Recognition is already active; the current session can continue.
  }
}

function stopRecognition(nextState: VoiceState = "paused") {
  shouldListen = false;
  window.clearTimeout(restartTimer);
  recognition?.stop();
  voiceState.value = nextState;
}

function startAudioMeter(stream: MediaStream) {
  audioContext?.close();
  window.cancelAnimationFrame(meterFrame);

  audioContext = new AudioContext();
  const analyser = audioContext.createAnalyser();
  const source = audioContext.createMediaStreamSource(stream);
  const samples = new Uint8Array(analyser.frequencyBinCount);
  analyser.fftSize = 256;
  source.connect(analyser);

  const update = () => {
    analyser.getByteFrequencyData(samples);
    const average = samples.reduce((sum, value) => sum + value, 0) / samples.length;
    audioLevel.value = Math.min(1, average / 55);
    meterFrame = window.requestAnimationFrame(update);
  };
  update();
}

async function countMicrophones() {
  if (!navigator.mediaDevices?.enumerateDevices) return;
  const devices = await navigator.mediaDevices.enumerateDevices();
  microphoneDevices.value = devices.filter((device) => device.kind === "audioinput").length;
}

async function requestMicrophone() {
  if (!navigator.mediaDevices?.getUserMedia) {
    microphonePermission.value = "unsupported";
    voiceState.value = "error";
    lastError.value = {
      title: "Микрофон недоступен",
      message: "В текущем окне отсутствует getUserMedia.",
      guidance: "Запустите полное приложение C-3PO, а не браузерный предпросмотр.",
      timestamp: new Date().toLocaleString("ru-RU"),
    };
    await writeLog("error", "microphone.request", lastError.value.message, {
      secureContext: window.isSecureContext,
      userAgent: navigator.userAgent,
    });
    return;
  }

  try {
    lastError.value = null;
    await writeLog("info", "microphone.request", "Пользователь запросил доступ к микрофону", {
      secureContext: window.isSecureContext,
      platform: environment.value?.platform ?? navigator.platform,
    });
    microphoneStream?.getTracks().forEach((track) => track.stop());
    microphoneStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    rememberMicrophonePermission("granted");
    microphoneActive.value = true;
    onboardingOpen.value = false;
    await countMicrophones();
    await writeLog("info", "microphone.request", "Доступ к микрофону получен", {
      audioInputs: microphoneDevices.value,
      tracks: microphoneStream.getAudioTracks().map((track) => ({
        label: track.label || "без названия",
        enabled: track.enabled,
        readyState: track.readyState,
      })),
    });
    startAudioMeter(microphoneStream);

    if (recognitionAvailable.value) startRecognition();
    else voiceState.value = "idle";
  } catch (error) {
    const details = errorDetails(error);
    lastError.value = microphoneErrorDescription(error);
    if (details.name === "NotAllowedError") rememberMicrophonePermission("denied");
    else microphonePermission.value = "unsupported";
    voiceState.value = "error";
    onboardingOpen.value = false;
    await writeLog("error", "microphone.request", `${details.name}: ${details.message}`, {
      stack: details.stack,
      secureContext: window.isSecureContext,
      platform: environment.value?.platform ?? navigator.platform,
      userAgent: navigator.userAgent,
      recognitionAvailable: recognitionAvailable.value,
    });
    await refreshLogs();
  }
}

async function inspectMicrophonePermission() {
  if (!navigator.mediaDevices?.getUserMedia) {
    microphonePermission.value = "unsupported";
    return;
  }

  const rememberedApproval = await hasRememberedMicrophoneApproval();
  let reportedState: PermissionState | null = null;
  try {
    const status = await navigator.permissions.query({ name: "microphone" as PermissionName });
    reportedState = status.state;
    status.onchange = () => {
      if (status.state === "granted") {
        rememberMicrophonePermission("granted");
        void requestMicrophone();
      } else if (status.state === "denied") {
        rememberMicrophonePermission("denied");
        pauseMicrophone();
      }
    };
  } catch (error) {
    const details = errorDetails(error);
    await writeLog("warn", "microphone.permission-check", `${details.name}: ${details.message}`);
  }

  await writeLog("info", "microphone.permission-memory", "Восстановление состояния микрофона", {
    rememberedApproval,
    reportedState,
  });

  if (reportedState === "granted" || (reportedState !== "denied" && rememberedApproval)) {
    // WKWebView может сообщать `prompt` при уже выданном разрешении. Реальный
    // getUserMedia является окончательной проверкой и не показывает окно снова,
    // если macOS сохранила доступ.
    microphonePermission.value = "granted";
    await requestMicrophone();
  } else if (reportedState === "denied") {
    rememberMicrophonePermission("denied");
    onboardingOpen.value = false;
  } else {
    microphonePermission.value = savedMicrophonePermission() === "denied" ? "denied" : "prompt";
    onboardingOpen.value = microphonePermission.value === "prompt";
  }
}

function pauseMicrophone() {
  stopRecognition("paused");
  window.speechSynthesis?.cancel();
  microphoneStream?.getTracks().forEach((track) => track.stop());
  microphoneStream = null;
  microphoneActive.value = false;
  window.cancelAnimationFrame(meterFrame);
  audioLevel.value = 0;
}

async function toggleMicrophone() {
  if (canPauseMicrophone.value) {
    pauseMicrophone();
    return;
  }
  await requestMicrophone();
}

function speak(text: string) {
  if (!synthesisAvailable.value) return;

  voiceState.value = "speaking";
  recognition?.stop();
  window.speechSynthesis.cancel();
  const utterance = new SpeechSynthesisUtterance(text);
  const voices = window.speechSynthesis.getVoices();
  // Голос выбирается по языку команд. Пользователь всегда может изменить
  // системный голос в настройках macOS или Windows.
  const voicePrefix = selectedCommandLanguage.value.speechCode.slice(0, 2).toLowerCase();
  const matchingVoices = voices.filter((voice) => voice.lang.toLowerCase().startsWith(voicePrefix));
  utterance.voice = matchingVoices[0] ?? null;
  selectedVoiceName.value = utterance.voice?.name ?? selectedCommandLanguage.value.nativeName;
  utterance.lang = selectedCommandLanguage.value.speechCode;
  utterance.rate = 0.9;
  utterance.pitch = 1.08;
  utterance.volume = 0.92;
  utterance.onend = () => {
    voiceState.value = "idle";
    if (microphonePermission.value === "granted") startRecognition();
  };
  utterance.onerror = () => {
    voiceState.value = "error";
  };
  window.speechSynthesis.speak(utterance);
}

function testVoice() {
  const phrase = commandLanguage.value === "ru"
    ? voiceDemoPhrases[voiceDemoIndex % voiceDemoPhrases.length]
    : commandText("greeting");
  voiceDemoIndex += 1;
  lastCommand.value = phrase;
  addMessage("assistant", phrase);
  speak(phrase);
}

function proposeSystemInfo() {
  pendingAction.value = true;
  voiceState.value = "idle";
  const response = `${moviePhrase("riskyAction")} Скажите «выполнить» или нажмите кнопку подтверждения.`;
  addMessage("assistant", response);
  speak(response);
}

function cancelAction() {
  pendingAction.value = false;
  const response = assistantPhrase("actionCancelled");
  addMessage("assistant", response);
  speak(response);
}

async function approveAction() {
  isWorking.value = true;
  voiceState.value = "thinking";

  try {
    const info = "__TAURI_INTERNALS__" in window
      ? await invoke<SystemInfo>("get_system_info")
      : environment.value?.system;

    if (!info) throw new Error("Сведения о системе пока недоступны");

    pendingAction.value = false;
    const memory = info.totalMemoryMb > 0 ? `${Math.round(info.totalMemoryMb / 1024)} ГБ памяти` : "объём памяти не определён";
    await refreshProcessMetrics();
    const applicationMemory = processMetrics.value
      ? `C-3PO сейчас использует ${processMemoryLabel.value} оперативной памяти`
      : "расход памяти C-3PO определить не удалось";
    const response = `${info.operatingSystem} ${info.osVersion}, архитектура ${info.architecture}, всего ${memory}. ${applicationMemory}.`;
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
  } catch (error) {
    const response = `${moviePhrase("criticalError")} ${String(error)}`;
    addMessage("assistant", response);
    speak(response);
  } finally {
    isWorking.value = false;
  }
}

function processVoiceCommand(text: string) {
  const normalized = text.toLowerCase();
  const localizedSystemCommand = commandText("systemCommand").toLowerCase();
  const localizedChatCommand = commandText("chatCommand").toLowerCase();
  lastCommand.value = text;
  liveTranscript.value = "";
  addMessage("user", text);

  if (awaitingIntroduction.value) {
    rememberIntroducedUser(text);
    return;
  }

  if (/(другой пользователь|новый пользователь|another user|new user|otro usuario|neuer benutzer|nouvel utilisateur|nuovo utente|novo usuário|新用户|別のユーザー|مستخدم جديد)/i.test(normalized)) {
    startNewSpeaker();
    return;
  }

  if (pendingAction.value && /(выполн|разреш|подтверж|да)/.test(normalized)) {
    void approveAction();
    return;
  }
  if (pendingAction.value && /(отмен|нет|стоп)/.test(normalized)) {
    cancelAction();
    return;
  }
  if (normalized.includes(localizedSystemCommand) || /(систем|компьютер|операцион|памят|процессор)/.test(normalized)) {
    proposeSystemInfo();
    return;
  }
  if (normalized.includes(localizedChatCommand) || /(открой|покажи).*(чат|диалог)/.test(normalized)) {
    chatOpen.value = true;
    const response = assistantPhrase("chatOpened");
    addMessage("assistant", response);
    speak(response);
    return;
  }
  if (/(закрой|скрой).*(чат|диалог)/.test(normalized)) {
    chatOpen.value = false;
    speak(assistantPhrase("chatClosed"));
    return;
  }

  if (llmStatus.value?.state === "ready") {
    void askLocalModel(text);
    return;
  }

  const response = `${moviePhrase("unknownBehavior")} Сейчас доступна информация о системе.`;
  addMessage("assistant", response);
  speak(response);
}

function submitMessage() {
  const text = input.value.trim();
  if (!text || isWorking.value) return;
  input.value = "";
  processVoiceCommand(text);
}

function permissionStatus(id: string) {
  if (id === "microphone") return microphonePermission.value;
  const capability = environment.value?.capabilities.find((item) => item.id === id);
  if (!capability?.available) return "unsupported";
  if (!capability.permissionRequired) return "granted";
  return "prompt";
}

function permissionStatusLabel(id: string) {
  const labels: Record<PermissionState, string> = {
    checking: "проверка",
    prompt: "нужно разрешение",
    granted: "разрешено",
    denied: "запрещено",
    unsupported: "недоступно",
  };
  return labels[permissionStatus(id) as PermissionState] ?? "неизвестно";
}

function saveInterfaceLanguage() {
  window.localStorage.setItem("c3po.interface-language", interfaceLanguage.value);
  document.documentElement.lang = interfaceLanguage.value;
  document.documentElement.dir = interfaceLanguage.value === "ar" ? "rtl" : "ltr";
  if (messages.value.length === 1) {
    messages.value[0].text = awaitingIntroduction.value
      ? introductionPrompt(commandLanguage.value)
      : `${t("greeting")} ${currentProfile.value?.name ?? ""}`.trim();
  }
  if (!liveTranscript.value) {
    lastCommand.value = awaitingIntroduction.value ? introductionPrompt(commandLanguage.value) : t("sayCommand");
  }
}

function saveCommandLanguage() {
  window.localStorage.setItem("c3po.command-language", commandLanguage.value);
  const resumeListening = shouldListen;
  if (recognition) {
    stopRecognition(resumeListening ? "idle" : "paused");
    recognition.lang = selectedCommandLanguage.value.speechCode;
    if (resumeListening) window.setTimeout(startRecognition, 200);
  }
  selectedVoiceName.value = selectedCommandLanguage.value.nativeName;
}

onMounted(async () => {
  saveInterfaceLanguage();
  saveCommandLanguage();
  await loadEnvironment();
  await loadLlmStatus();
  configureRecognition();
  await refreshProcessMetrics();
  // Снимаем лёгкий замер каждые пять секунд. Базовое и пиковое значения
  // позволяют заметить устойчивый рост памяти во время долгой работы.
  memoryTimer = window.setInterval(refreshProcessMetrics, 5_000);
  await inspectMicrophonePermission();
});

onBeforeUnmount(() => {
  window.clearInterval(memoryTimer);
  pauseMicrophone();
  window.speechSynthesis?.cancel();
  audioContext?.close();
  droidAudioContext?.close();
});
</script>

<template>
  <div class="voice-app" :class="`state-${voiceState}`">
    <header class="app-header">
      <div class="compact-brand">
        <div class="brand-symbol">C</div>
        <div>
          <strong>C-3PO</strong>
          <span>{{ t("localAssistant") }}</span>
        </div>
      </div>

      <div class="environment-chip">
        <span class="status-light"></span>
        <div>
          <strong>{{ systemLabel }}</strong>
          <span>
            {{ environment?.system.architecture || "Проверка архитектуры" }} · C-3PO {{ processMemoryLabel }} RAM
          </span>
        </div>
      </div>

      <div class="header-actions">
        <button class="ghost-button model-button" type="button" :aria-label="llmShortLabel" @click="openLlmPanel">
          <span class="button-icon">◈</span>
          <span>{{ llmShortLabel }}</span>
          <i :class="llmStatus?.state === 'ready' ? 'granted' : 'prompt'"></i>
        </button>
        <button class="ghost-button language-button" type="button" :aria-label="t('languageSettings')" @click="languageOpen = true">
          <span class="button-icon">文</span>
          <span>{{ interfaceLanguage.toUpperCase() }} · {{ commandLanguage.toUpperCase() }}</span>
        </button>
        <button class="ghost-button" type="button" :aria-label="t('permissions')" @click="openPermissions">
          <span class="button-icon">⌾</span>
          <span>{{ t("permissions") }}</span>
          <i :class="microphonePermission"></i>
        </button>
        <button class="chat-button" type="button" :aria-label="t('openChat')" @click="chatOpen = true">
          <span>{{ t("openChat") }}</span>
          <b>{{ Math.max(0, messages.length - 1) }}</b>
        </button>
      </div>
    </header>

    <main class="voice-workspace">
      <section class="voice-hero">
        <div class="ambient-grid"></div>
        <div class="droid-console" :style="{ '--voice-level': audioLevel }">
          <div class="scanner-ring ring-one"></div>
          <div class="scanner-ring ring-two"></div>
          <div class="voice-halo"></div>
          <img :src="droidPortrait" alt="Золотой протокольный дроид C-3PO" />
          <div class="voice-spectrum" aria-hidden="true">
            <i v-for="index in 18" :key="index" :style="{ '--index': index }"></i>
          </div>
        </div>

        <div class="voice-copy">
          <span class="eyebrow">{{ t("voiceMode") }}</span>
          <h1>{{ voiceStatus }}</h1>
          <p class="transcript">{{ liveTranscript || lastCommand }}</p>

          <button
            class="primary-microphone"
            :class="{ active: microphoneActive }"
            type="button"
            @click="toggleMicrophone"
          >
            <span class="microphone-icon"></span>
            <span>{{ canPauseMicrophone ? t("pauseMicrophone") : t("enableMicrophone") }}</span>
          </button>

          <p v-if="microphonePermission === 'granted' && !recognitionAvailable" class="voice-warning">
            {{ recognitionBlockedReason || "Микрофон доступен, но распознавание речи сейчас отключено." }}
          </p>
          <div v-if="lastError" class="error-card" role="alert">
            <div>
              <strong>{{ lastError.title }}</strong>
              <small>{{ lastError.timestamp }}</small>
            </div>
            <p>{{ lastError.message }}</p>
            <p>{{ lastError.guidance }}</p>
            <button type="button" @click="openPermissions">Открыть журнал</button>
          </div>
        </div>
      </section>

      <section class="quick-panel">
        <div class="section-heading">
          <div>
            <span class="eyebrow">{{ t("quickStart") }}</span>
            <h2>{{ t("whatCanSay") }}</h2>
          </div>
          <span class="privacy-badge">{{ t("localOnly") }}</span>
        </div>

        <div class="command-grid">
          <button type="button" @click="proposeSystemInfo">
            <span class="command-icon">⌘</span>
            <span>
              <strong>«{{ t("systemCommand") }}»</strong>
              <small>{{ t("systemDescription") }}</small>
            </span>
            <b>→</b>
          </button>
          <button type="button" @click="chatOpen = true">
            <span class="command-icon">◇</span>
            <span>
              <strong>«{{ t("chatCommand") }}»</strong>
              <small>{{ t("chatDescription") }}</small>
            </span>
            <b>→</b>
          </button>
          <button type="button" @click="testVoice">
            <span class="command-icon">♫</span>
            <span>
              <strong>{{ t("testVoice") }}</strong>
              <small>{{ selectedVoiceName }} · {{ t("voiceDescription") }}</small>
            </span>
            <b>▶</b>
          </button>
        </div>
      </section>

      <section v-if="pendingAction" class="approval-bar">
        <div class="approval-icon">⌘</div>
        <div>
          <span class="eyebrow">НУЖНО ПОДТВЕРЖДЕНИЕ</span>
          <h3>Прочитать информацию о системе?</h3>
          <p>Файлы и настройки не изменяются.</p>
        </div>
        <div class="approval-actions">
          <button type="button" @click="cancelAction">Отмена</button>
          <button class="approve-button" type="button" :disabled="isWorking" @click="approveAction">
            {{ isWorking ? "Выполняю…" : "Разрешить" }}
          </button>
        </div>
      </section>
    </main>

    <div v-if="chatOpen" class="drawer-backdrop" @click.self="chatOpen = false">
      <aside class="chat-drawer" :aria-label="t('dialogTitle')">
        <header>
          <div class="drawer-title">
            <span class="drawer-avatar">C</span>
            <div>
              <span class="eyebrow">{{ t("history") }}</span>
              <h2>{{ t("dialogTitle") }}</h2>
            </div>
          </div>
          <div class="drawer-header-actions">
            <button
              type="button"
              :class="{ active: conversationListOpen }"
              :aria-label="dialogLabels[interfaceLanguage].list"
              @click="conversationListOpen = !conversationListOpen"
            >☷</button>
            <button type="button" :aria-label="t('close')" @click="chatOpen = false">×</button>
          </div>
        </header>

        <div class="chat-context">
          <span class="context-light"></span>
          <div>
            <strong>C-3PO ↔ {{ currentProfile?.name || "…" }}</strong>
            <small>{{ profileLabels[interfaceLanguage].localMemory }}</small>
          </div>
          <b>{{ commandLanguage.toUpperCase() }}</b>
        </div>

        <section v-if="conversationListOpen" class="conversation-browser">
          <header>
            <div>
              <span class="eyebrow">{{ t("history") }}</span>
              <h3>{{ dialogLabels[interfaceLanguage].list }}</h3>
            </div>
            <button type="button" @click="startNewConversation">＋ {{ dialogLabels[interfaceLanguage].newDialog }}</button>
          </header>
          <p v-if="!conversationThreads.length" class="conversation-empty">
            {{ dialogLabels[interfaceLanguage].empty }}
          </p>
          <button
            v-for="thread in conversationThreads"
            :key="thread.id"
            class="conversation-item"
            :class="{ active: thread.id === activeConversationId }"
            type="button"
            @click="openConversation(thread)"
          >
            <span>◇</span>
            <span><strong>{{ thread.title }}</strong><small>{{ formatConversationDate(thread.updatedAt) }}</small></span>
            <b>→</b>
          </button>
        </section>

        <div v-else ref="conversation" class="drawer-messages">
          <div v-for="message in messages" :key="message.id" class="drawer-message" :class="message.role">
            <span v-if="message.role === 'assistant'" class="small-avatar">C</span>
            <p>{{ message.text }}</p>
          </div>
        </div>

        <form v-if="!conversationListOpen" class="drawer-composer" @submit.prevent="submitMessage">
          <span class="composer-language">{{ commandLanguage.toUpperCase() }}</span>
          <textarea
            v-model="input"
            rows="2"
            :placeholder="t('enterCommand')"
            :aria-label="t('enterCommand')"
            @keydown.enter.exact.prevent="submitMessage"
          ></textarea>
          <button type="submit" :disabled="!canSend">↑</button>
        </form>
      </aside>
    </div>

    <div v-if="languageOpen" class="modal-backdrop" @click.self="languageOpen = false">
      <section class="permission-modal language-modal" :aria-label="t('languageSettings')">
        <header>
          <div class="language-title">
            <span class="language-emblem">文</span>
            <div>
            <span class="eyebrow">{{ t("language") }}</span>
            <h2>{{ t("languageSettings") }}</h2>
            </div>
          </div>
          <button type="button" :aria-label="t('close')" @click="languageOpen = false">×</button>
        </header>

        <div class="language-fields">
          <label class="language-card">
            <span class="language-card-icon">Aa</span>
            <span class="language-card-copy">
              <strong>{{ t("interfaceLanguage") }}</strong>
              <small>{{ languages.find((item) => item.code === interfaceLanguage)?.nativeName }}</small>
            </span>
            <span class="select-shell">
              <select v-model="interfaceLanguage" @change="saveInterfaceLanguage">
                <option v-for="language in languages" :key="language.code" :value="language.code">
                  {{ language.nativeName }}
                </option>
              </select>
            </span>
          </label>
          <label class="language-card">
            <span class="language-card-icon voice-language-icon">◌</span>
            <span class="language-card-copy">
              <strong>{{ t("commandLanguage") }}</strong>
              <small>{{ selectedCommandLanguage.speechCode }} · R2-D2</small>
            </span>
            <span class="select-shell">
              <select v-model="commandLanguage" @change="saveCommandLanguage">
                <option v-for="language in languages" :key="language.code" :value="language.code">
                  {{ language.nativeName }}
                </option>
              </select>
            </span>
          </label>
        </div>

        <div class="profile-settings">
          <div class="profile-heading">
            <span class="profile-avatar">{{ currentProfile?.name?.charAt(0).toUpperCase() || "?" }}</span>
            <div>
              <strong>{{ profileLabels[interfaceLanguage].current }}</strong>
              <small>{{ profileLabels[interfaceLanguage].localMemory }}</small>
            </div>
          </div>
          <label v-if="profiles.length">
            <select v-model="currentProfileId" @change="switchProfile">
              <option v-for="profile in profiles" :key="profile.id" :value="profile.id">{{ profile.name }}</option>
            </select>
          </label>
          <button type="button" @click="startNewSpeaker">
            <span>＋</span>{{ profileLabels[interfaceLanguage].newUser }}
          </button>
        </div>

        <div class="language-actions">
          <small><span>✓</span>{{ t("languageHint") }}</small>
          <button class="approve-button" type="button" @click="languageOpen = false">{{ t("done") }}</button>
        </div>
      </section>
    </div>

    <div v-if="permissionsOpen" class="modal-backdrop" @click.self="permissionsOpen = false">
      <section class="permission-modal" aria-label="Доступы приложения">
        <header>
          <div>
            <span class="eyebrow">СИСТЕМНЫЕ РАЗРЕШЕНИЯ</span>
            <h2>Что доступно C-3PO</h2>
          </div>
          <button type="button" aria-label="Закрыть" @click="permissionsOpen = false">×</button>
        </header>

        <div class="system-summary">
          <div><span>Операционная система</span><strong>{{ systemLabel }}</strong></div>
          <div><span>Архитектура</span><strong>{{ environment?.system.architecture || "—" }}</strong></div>
          <div><span>Компьютер</span><strong>{{ environment?.system.hostName || "—" }}</strong></div>
          <div>
            <span>Память C-3PO (RSS)</span>
            <strong>{{ processMemoryLabel }}</strong>
            <small>От открытия: {{ processMemoryDelta }} · пик {{ formatBytes(memoryPeakBytes) }}</small>
          </div>
        </div>

        <div class="permission-list">
          <article>
            <span class="permission-icon">●</span>
            <div><strong>Микрофон</strong><small>{{ microphoneLabel }}</small></div>
            <span class="permission-state" :class="permissionStatus('microphone')">
              {{ permissionStatusLabel("microphone") }}
            </span>
            <button v-if="microphonePermission !== 'granted'" type="button" @click="requestMicrophone">Запросить</button>
          </article>
          <article>
            <span class="permission-icon">⌘</span>
            <div><strong>Информация о системе</strong><small>Только чтение основных сведений</small></div>
            <span class="permission-state granted">доступно</span>
          </article>
          <article>
            <span class="permission-icon">▱</span>
            <div><strong>Файлы и папки</strong><small>Будет запрашиваться для выбранной папки</small></div>
            <span class="permission-state prompt">по запросу</span>
          </article>
          <article>
            <span class="permission-icon">▣</span>
            <div><strong>Экран и управление</strong><small>Текущим функциям не требуется</small></div>
            <span class="permission-state neutral">не запрошено</span>
          </article>
        </div>

        <section class="error-log" aria-label="Журнал ошибок">
          <header>
            <div>
              <strong>Журнал событий и ошибок</strong>
              <small>{{ logSummary }} · звук и текст команд не сохраняются</small>
            </div>
            <div class="log-actions">
              <button type="button" :disabled="logsLoading" @click="refreshLogs" title="Обновить журнал">↻</button>
              <button type="button" @click="logsExpanded = !logsExpanded">
                {{ logsExpanded ? "Свернуть" : "Показать" }}
              </button>
            </div>
          </header>
          <template v-if="logsExpanded">
            <pre v-if="recentLogs.length">{{ recentLogs.join("\n") }}</pre>
            <p v-else>Записей пока нет.</p>
            <small v-if="logPath" class="log-path">Файл: {{ logPath }}</small>
          </template>
        </section>

        <p class="permission-note">
          C-3PO не запрашивает права заранее, если функция ими не пользуется. Это уменьшает доступ
          приложения к системе.
        </p>
      </section>
    </div>

    <div v-if="llmOpen" class="modal-backdrop" @click.self="llmOpen = false">
      <section class="permission-modal model-modal" aria-label="Локальная модель">
        <header>
          <div>
            <span class="eyebrow">ЛОКАЛЬНЫЙ ИНТЕЛЛЕКТ</span>
            <h2>R2-D2 — локальная модель</h2>
          </div>
          <button type="button" aria-label="Закрыть" @click="llmOpen = false">×</button>
        </header>

        <div class="model-state" :class="llmStatus?.state || 'checking'">
          <span class="model-state-icon">◈</span>
          <div>
            <strong>{{ llmShortLabel }}</strong>
            <p>{{ llmStatus?.message || "Проверяю локальное окружение…" }}</p>
          </div>
        </div>

        <div class="model-checklist">
          <article>
            <span>1</span>
            <div>
              <strong>Движок llama.cpp</strong>
              <small>{{ llmStatus?.enginePath || "Не найден" }}</small>
            </div>
            <b :class="llmStatus?.engineFound ? 'ready' : 'missing'">
              {{ llmStatus?.engineFound ? "найден" : "нужен" }}
            </b>
          </article>
          <article>
            <span>2</span>
            <div>
              <strong>Модель GGUF</strong>
              <small v-if="llmStatus?.selectedModel">
                {{ llmStatus.selectedModel.name }} · {{ formatBytes(llmStatus.selectedModel.sizeBytes) }}
              </small>
              <small v-else>Пока не установлена</small>
            </div>
            <b :class="llmStatus?.selectedModel ? 'ready' : 'missing'">
              {{ llmStatus?.selectedModel ? "готова" : "нужна" }}
            </b>
          </article>
          <article>
            <span>3</span>
            <div>
              <strong>Локальная генерация</strong>
              <small>Запуск сервера и потоковые ответы</small>
            </div>
            <b>следующий шаг</b>
          </article>
          <article>
            <span>4</span>
            <div>
              <strong>Состояние процесса</strong>
              <small>
                {{ llmRuntime?.message || "Модель ещё не запускалась" }}
                <template v-if="llmRuntime?.processId">
                  · PID {{ llmRuntime.processId }} · RAM {{ formatBytes(llmRuntime.residentMemoryBytes) }}
                </template>
              </small>
            </div>
            <b :class="llmRuntime?.ready ? 'ready' : ''">{{ llmRuntime?.state || "остановлена" }}</b>
          </article>
        </div>

        <div class="model-path">
          <span>Папка для моделей</span>
          <code>{{ llmStatus?.modelDirectory || "—" }}</code>
          <small>Файл модели должен иметь расширение .gguf и корректный заголовок GGUF.</small>
        </div>

        <div class="model-actions">
          <button type="button" :disabled="llmLoading" @click="loadLlmStatus">Обновить</button>
          <button v-if="llmRuntime?.ready" type="button" :disabled="llmLoading" @click="stopLocalModel">
            Остановить
          </button>
          <button
            v-else
            class="model-primary-action"
            type="button"
            :disabled="llmLoading || llmStatus?.state !== 'ready'"
            @click="startLocalModel"
          >
            {{ llmLoading ? "Загружаю…" : "Запустить модель" }}
          </button>
          <button
            class="model-primary-action"
            type="button"
            :disabled="llmLoading || llmStatus?.state !== 'ready'"
            @click="testLocalModel"
          >Тестовый вопрос</button>
        </div>
        <div v-if="llmTestReply" class="model-test-reply">
          <span>Ответ, подготовленный для C-3PO</span>
          <p>{{ llmTestReply }}</p>
        </div>
      </section>
    </div>

    <div v-if="onboardingOpen" class="modal-backdrop onboarding-backdrop">
      <section class="permission-modal onboarding-card" aria-label="Первичная настройка микрофона">
        <div class="onboarding-visual">
          <img :src="droidPortrait" alt="C-3PO" />
          <div class="onboarding-pulse"></div>
        </div>
        <span class="eyebrow">ПЕРВЫЙ ЗАПУСК</span>
        <h2>Разрешить голосовые команды?</h2>
        <p>
          C-3PO использует микрофон только пока голосовой режим включён. macOS или Windows покажет
          собственное системное окно подтверждения.
        </p>
        <ul>
          <li>Вы всегда видите, когда микрофон активен</li>
          <li>Микрофон можно поставить на паузу одним нажатием</li>
          <li>Остальные разрешения запрашиваются только при необходимости</li>
        </ul>
        <div class="onboarding-actions">
          <button type="button" @click="onboardingOpen = false; voiceState = 'paused'">Пока не сейчас</button>
          <button class="approve-button" type="button" @click="requestMicrophone">Разрешить микрофон</button>
        </div>
      </section>
    </div>
  </div>
</template>
