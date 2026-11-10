<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import {
  isPermissionGranted as isNotificationPermissionGranted,
  requestPermission as requestNotificationPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import droidPortrait from "./assets/protocol-droid.png";
import { assistantPhrase, moviePhrase } from "./assistantPhrases";
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
  role: "general" | "code" | "writing-mail";
};

type DroidModelProfile = {
  role: LocalModelFile["role"];
  droid: "R2-D2" | "BB-8" | "K-2SO";
  purpose: string;
  recommendedModel: string;
  model: LocalModelFile | null;
};

type LlmStatus = {
  state: "engine-missing" | "model-missing" | "model-invalid" | "ready";
  engineFound: boolean;
  enginePath: string | null;
  modelDirectory: string;
  selectedModel: LocalModelFile | null;
  models?: LocalModelFile[];
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

type TtsStatus = {
  state: "runtime-missing" | "reference-missing" | "stopped" | "starting" | "running";
  runtimeFound: boolean;
  referenceFound: boolean;
  processId: number | null;
  address: string;
  dataDirectory: string;
  referencePath: string | null;
  message: string;
};

type LocalReply = {
  text: string;
  model?: string;
  task?: string;
};

type ModelInstallReport = {
  installed: string[];
  skipped: string[];
  downloadedBytes: number;
  message: string;
};

type PrivateVoicePhrase = {
  id: string;
  category: "startup" | "success" | "warning" | "error" | "waiting" | "introduction" | "shutdown" | "general";
  text: string;
  file: string;
  recommended: boolean;
};

type PrivateVoiceCatalog = {
  version: number;
  private: true;
  phrases: PrivateVoicePhrase[];
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
type ThemePreference = "system" | "light" | "dark";
type VoiceMode = "adaptive" | "instant" | "personal";

type AppError = {
  title: string;
  message: string;
  guidance: string;
  timestamp: string;
};

type PendingAction =
  | { kind: "system-info" }
  | { kind: "telegram-message"; contact: string; message: string };

type NativeActionResult = {
  success: boolean;
  message: string;
};

type WeatherReport = {
  source: string;
  summary: string;
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
  onspeechstart: (() => void) | null;
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
const storedTheme = window.localStorage.getItem("c3po.theme");
const themePreference = ref<ThemePreference>(storedTheme === "light" || storedTheme === "dark" ? storedTheme : "system");
const silentMode = ref(window.localStorage.getItem("c3po.silent-mode") === "true");
const storedVoiceMode = window.localStorage.getItem("c3po.voice-mode");
const voiceMode = ref<VoiceMode>(
  storedVoiceMode === "instant" || storedVoiceMode === "personal" ? storedVoiceMode : "adaptive",
);
const systemDarkMode = window.matchMedia("(prefers-color-scheme: dark)");

function applyTheme() {
  const resolved = themePreference.value === "system"
    ? (systemDarkMode.matches ? "dark" : "light")
    : themePreference.value;
  document.documentElement.dataset.theme = resolved;
  document.documentElement.style.colorScheme = resolved;
  window.localStorage.setItem("c3po.theme", themePreference.value);
}

applyTheme();

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

const dialogLabels: Record<LanguageCode, { list: string; newDialog: string; empty: string; clear: string; deleteDialog: string }> = {
  ru: { list: "История диалогов", newDialog: "Новый диалог", empty: "Диалогов пока нет", clear: "Очистить всё", deleteDialog: "Удалить диалог" },
  en: { list: "Conversation history", newDialog: "New conversation", empty: "No conversations yet", clear: "Clear all", deleteDialog: "Delete conversation" },
  es: { list: "Historial de conversaciones", newDialog: "Nueva conversación", empty: "Aún no hay conversaciones", clear: "Borrar todo", deleteDialog: "Eliminar conversación" },
  de: { list: "Gesprächsverlauf", newDialog: "Neues Gespräch", empty: "Noch keine Gespräche", clear: "Alle löschen", deleteDialog: "Gespräch löschen" },
  fr: { list: "Historique des dialogues", newDialog: "Nouveau dialogue", empty: "Aucun dialogue", clear: "Tout effacer", deleteDialog: "Supprimer le dialogue" },
  it: { list: "Cronologia dialoghi", newDialog: "Nuovo dialogo", empty: "Nessun dialogo", clear: "Cancella tutto", deleteDialog: "Elimina dialogo" },
  pt: { list: "Histórico de conversas", newDialog: "Nova conversa", empty: "Nenhuma conversa", clear: "Limpar tudo", deleteDialog: "Excluir conversa" },
  zh: { list: "对话历史", newDialog: "新对话", empty: "暂无对话", clear: "全部清除", deleteDialog: "删除对话" },
  ja: { list: "会話履歴", newDialog: "新しい会話", empty: "会話はまだありません", clear: "すべて消去", deleteDialog: "会話を削除" },
  ar: { list: "سجل المحادثات", newDialog: "محادثة جديدة", empty: "لا توجد محادثات بعد", clear: "مسح الكل", deleteDialog: "حذف المحادثة" },
};

const settingsLabels: Record<LanguageCode, { settings: string; appearance: string; system: string; light: string; dark: string; background: string; wake: string; active: string }> = {
  ru: { settings: "Настройки", appearance: "Оформление", system: "Как в системе", light: "Светлая", dark: "Тёмная", background: "Фоновый режим", wake: "Пробуждение по имени C-3PO", active: "Активно" },
  en: { settings: "Settings", appearance: "Appearance", system: "System", light: "Light", dark: "Dark", background: "Background mode", wake: "Wake with the name C-3PO", active: "Active" },
  es: { settings: "Ajustes", appearance: "Apariencia", system: "Sistema", light: "Claro", dark: "Oscuro", background: "Modo en segundo plano", wake: "Activar con el nombre C-3PO", active: "Activo" },
  de: { settings: "Einstellungen", appearance: "Darstellung", system: "System", light: "Hell", dark: "Dunkel", background: "Hintergrundmodus", wake: "Mit C-3PO aktivieren", active: "Aktiv" },
  fr: { settings: "Réglages", appearance: "Apparence", system: "Système", light: "Clair", dark: "Sombre", background: "Mode arrière-plan", wake: "Activation avec C-3PO", active: "Actif" },
  it: { settings: "Impostazioni", appearance: "Aspetto", system: "Sistema", light: "Chiaro", dark: "Scuro", background: "Modalità in background", wake: "Attiva con C-3PO", active: "Attivo" },
  pt: { settings: "Configurações", appearance: "Aparência", system: "Sistema", light: "Claro", dark: "Escuro", background: "Modo em segundo plano", wake: "Ativar com C-3PO", active: "Ativo" },
  zh: { settings: "设置", appearance: "外观", system: "跟随系统", light: "浅色", dark: "深色", background: "后台模式", wake: "通过 C-3PO 唤醒", active: "已启用" },
  ja: { settings: "設定", appearance: "外観", system: "システム", light: "ライト", dark: "ダーク", background: "バックグラウンドモード", wake: "C-3POで起動", active: "有効" },
  ar: { settings: "الإعدادات", appearance: "المظهر", system: "النظام", light: "فاتح", dark: "داكن", background: "وضع الخلفية", wake: "التنشيط باسم C-3PO", active: "نشط" },
};

const silentModeLabels: Record<LanguageCode, { title: string; description: string }> = {
  ru: { title: "Беззвучный режим", description: "Только текстовый чат, без микрофона и озвучки" },
  en: { title: "Silent mode", description: "Text chat only, without microphone or speech" },
  es: { title: "Modo silencioso", description: "Solo chat de texto, sin micrófono ni voz" },
  de: { title: "Stiller Modus", description: "Nur Textchat, ohne Mikrofon und Sprache" },
  fr: { title: "Mode silencieux", description: "Dialogue texte uniquement, sans microphone ni voix" },
  it: { title: "Modalità silenziosa", description: "Solo chat testuale, senza microfono né voce" },
  pt: { title: "Modo silencioso", description: "Somente chat de texto, sem microfone ou voz" },
  zh: { title: "静音模式", description: "仅文字聊天，不使用麦克风和语音" },
  ja: { title: "サイレントモード", description: "マイクと音声を使わずテキストチャットのみ" },
  ar: { title: "الوضع الصامت", description: "دردشة نصية فقط دون ميكروفون أو صوت" },
};

const voiceModeLabels: Record<LanguageCode, { title: string; adaptive: string; instant: string; personal: string }> = {
  ru: { title: "Режим голоса", adaptive: "Авто", instant: "Быстрый", personal: "Мой голос" },
  en: { title: "Voice mode", adaptive: "Auto", instant: "Fast", personal: "My voice" },
  es: { title: "Modo de voz", adaptive: "Auto", instant: "Rápido", personal: "Mi voz" },
  de: { title: "Sprachmodus", adaptive: "Auto", instant: "Schnell", personal: "Meine Stimme" },
  fr: { title: "Mode vocal", adaptive: "Auto", instant: "Rapide", personal: "Ma voix" },
  it: { title: "Modalità voce", adaptive: "Auto", instant: "Rapida", personal: "La mia voce" },
  pt: { title: "Modo de voz", adaptive: "Auto", instant: "Rápido", personal: "Minha voz" },
  zh: { title: "语音模式", adaptive: "自动", instant: "快速", personal: "我的声音" },
  ja: { title: "音声モード", adaptive: "自動", instant: "高速", personal: "自分の声" },
  ar: { title: "وضع الصوت", adaptive: "تلقائي", instant: "سريع", personal: "صوتي" },
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
  void scrollChat("auto");
}

function persistConversationThreads() {
  if (!currentProfileId.value) return;
  window.localStorage.setItem(
    `c3po.conversations.${currentProfileId.value}`,
    JSON.stringify(conversationThreads.value),
  );
}

function showMostRecentConversation() {
  const recent = conversationThreads.value[0];
  activeConversationId.value = recent?.id ?? "";
  messages.value = recent?.messages.length
    ? recent.messages
    : [{ id: 1, role: "assistant", text: `${t("greeting")} ${currentProfile.value?.name ?? ""}.`.trim() }];
  nextMessageId.value = Math.max(1, ...messages.value.map((message) => message.id)) + 1;
}

function deleteConversation(thread: ConversationThread) {
  if (!window.confirm(`${dialogLabels[interfaceLanguage.value].deleteDialog}: «${thread.title}»?`)) return;
  conversationThreads.value = conversationThreads.value.filter((item) => item.id !== thread.id);
  if (activeConversationId.value === thread.id) showMostRecentConversation();
  persistConversationThreads();
}

function clearAllConversations() {
  if (!conversationThreads.value.length) return;
  if (!window.confirm(`${dialogLabels[interfaceLanguage.value].clear}?`)) return;
  conversationThreads.value = [];
  showMostRecentConversation();
  persistConversationThreads();
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
const selectedFileContext = ref("");
const selectedFileNames = ref<string[]>([]);
const liveTranscript = ref("");
const lastCommand = ref(initialProfile ? t("sayCommand") : introductionPrompt(commandLanguage.value));
const pendingAction = ref<PendingAction | null>(null);
const awaitingTelegramMessageContact = ref("");
const isWorking = ref(false);
const isConsultingR2 = ref(false);
const chatOpen = ref(false);
const permissionsOpen = ref(false);
const llmOpen = ref(false);
const languageOpen = ref(false);
const settingsOpen = ref(false);
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
watch([chatOpen, conversationListOpen, activeConversationId], ([isOpen, listIsOpen]) => {
  if (isOpen) {
    // Текстовый диалог работает тихо. Если панель открыли во время ответа,
    // сразу прекращаем голос и оставляем пользователю точный текст.
    if (voiceState.value === "speaking") interruptAssistantSpeech("chat-opened");
    if (!listIsOpen) void scrollChat("auto");
  }
}, { flush: "post" });
watch(isConsultingR2, (isActive) => {
  // Временная реплика R2-D2 является частью диалога, поэтому при её появлении
  // прокручиваем историю вниз так же, как для обычного нового сообщения.
  if (isActive && chatOpen.value && !conversationListOpen.value) void scrollChat("smooth");
}, { flush: "post" });
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
const ttsStatus = ref<TtsStatus | null>(null);
const llmTestReply = ref("");
const modelInstallMessage = ref("");
const modelPreference = ref(window.localStorage.getItem("c3po.model-preference") || "auto");
const privateVoiceCatalog = ref<PrivateVoiceCatalog | null>(null);

let recognition: RecognitionLike | null = null;
let microphoneStream: MediaStream | null = null;
let audioContext: AudioContext | null = null;
let droidAudioContext: AudioContext | null = null;
let meterFrame = 0;
let restartTimer = 0;
let shouldListen = false;
let memoryTimer = 0;
let voiceDemoIndex = 0;
let privateVoiceIndex = 0;
let privateVoiceAudio: HTMLAudioElement | null = null;
let generatedVoiceAudio: HTMLAudioElement | null = null;
let generatedVoiceUrl = "";
let speechRunId = 0;
let stopGeneratedSpeech: (() => void) | null = null;
let stopPrivateVoiceSpeech: (() => void) | null = null;
let assistantSpeechStartedAt = 0;
let bargeInStartedAt = 0;
let privateVoicePlaying = false;
const notificationPermission = ref<"granted" | "denied" | "prompt">("prompt");
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
    ready: modelPreference.value === "auto" ? "Дроиды готовы" : `${selectedDroidName.value} готов`,
  };
  return labels[llmStatus.value.state];
});
const droidModelDefinitions: Array<Omit<DroidModelProfile, "model">> = [
  { role: "general", droid: "R2-D2", purpose: "Общие вопросы и быстрые команды", recommendedModel: "Qwen2.5 1.5B" },
  { role: "code", droid: "BB-8", purpose: "Программирование и разбор ошибок", recommendedModel: "Qwen2.5 Coder 3B" },
  { role: "writing-mail", droid: "K-2SO", purpose: "Тексты, документы и анализ почты", recommendedModel: "Qwen3 4B" },
];
const droidModelFleet = computed<DroidModelProfile[]>(() => {
  const installed = llmStatus.value?.models ?? (llmStatus.value?.selectedModel ? [llmStatus.value.selectedModel] : []);
  return droidModelDefinitions.map((definition) => ({
    ...definition,
    model: installed
      .filter((model) => model.role === definition.role && model.validGguf)
      .sort((left, right) => right.sizeBytes - left.sizeBytes)[0] ?? null,
  }));
});
const selectedDroidName = computed(() => {
  if (modelPreference.value === "auto") return "Автовыбор";
  return droidModelFleet.value.find((profile) => profile.model?.name === modelPreference.value)?.droid ?? "Автовыбор";
});
const pendingActionTitle = computed(() => {
  if (pendingAction.value?.kind === "telegram-message") {
    return `Отправить сообщение контакту «${pendingAction.value.contact}»?`;
  }
  return "Прочитать информацию о системе?";
});
const pendingActionDescription = computed(() => {
  if (pendingAction.value?.kind === "telegram-message") return pendingAction.value.message;
  return "Файлы и настройки не изменяются.";
});
const pendingActionButton = computed(() => pendingAction.value?.kind === "telegram-message" ? "Отправить" : "Разрешить");

function formatBytes(bytes: number) {
  const absolute = Math.abs(bytes);
  if (absolute < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} КБ`;
  if (absolute >= 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} ГБ`;
  return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
}

async function ensureNotificationPermission() {
  if (!("__TAURI_INTERNALS__" in window)) return false;
  try {
    if (await isNotificationPermissionGranted()) {
      notificationPermission.value = "granted";
      return true;
    }
    const state = await requestNotificationPermission();
    notificationPermission.value = state === "granted" ? "granted" : "denied";
    return state === "granted";
  } catch (error) {
    await writeLog("warn", "notification.permission", `Не удалось запросить уведомления: ${String(error)}`);
    return false;
  }
}

async function notifyFromBackground(body: string) {
  if (document.visibilityState === "visible" || !(await ensureNotificationPermission())) return;
  sendNotification({
    title: "C-3PO",
    body: body.trim().slice(0, 500),
  });
  await writeLog("info", "notification.background", "Ответ показан системным уведомлением");
}

async function loadPrivateVoiceCatalog() {
  for (const catalogUrl of ["./private-audio/catalog.json", "/private-audio/catalog.json"]) {
    try {
      const response = await fetch(catalogUrl, { cache: "no-store" });
      if (!response.ok) continue;
      privateVoiceCatalog.value = await response.json() as PrivateVoiceCatalog;
      await writeLog("info", "voice.private-pack", "Локальный голосовой пакет загружен", {
        catalogUrl,
        phrases: privateVoiceCatalog.value.phrases.length,
      });
      return;
    } catch (error) {
      await writeLog("warn", "voice.private-pack", `Не удалось загрузить каталог ${catalogUrl}: ${String(error)}`);
    }
  }
  // Пакет необязателен для публичной сборки, но причина его отсутствия всегда
  // остаётся в локальном журнале и больше не теряется молча.
  await writeLog("warn", "voice.private-pack", "Локальный каталог voice.mp3 не найден в сборке");
}

async function playPrivateVoice(
  category: PrivateVoicePhrase["category"],
  replaceCaption = true,
  waitForCompletion = false,
) {
  if (silentMode.value || chatOpen.value || commandLanguage.value !== "ru" || !privateVoiceCatalog.value) {
    await writeLog("info", "voice.private-pack", "Фраза пропущена", {
      category,
      silentMode: silentMode.value,
      commandLanguage: commandLanguage.value,
      catalogLoaded: Boolean(privateVoiceCatalog.value),
    });
    return false;
  }
  const candidates = privateVoiceCatalog.value.phrases.filter(
    (phrase) => phrase.category === category && phrase.recommended,
  );
  if (!candidates.length) return false;
  const phrase = candidates[privateVoiceIndex % candidates.length];
  privateVoiceIndex += 1;
  privateVoiceAudio?.pause();
  stopGeneratedSpeech?.();
  privateVoiceAudio = new Audio(`./private-audio/${phrase.file}`);
  privateVoiceAudio.preload = "auto";
  privateVoiceAudio.autoplay = true;
  privateVoiceAudio.volume = 0.9;
  // Показываем произносимую фразу сразу: пользователь видит, что именно
  // выбрал C-3PO, даже если системный аудиовыход временно недоступен.
  if (replaceCaption) lastCommand.value = phrase.text;
  const resumeListening = shouldListen;
  privateVoicePlaying = true;
  // Не выключаем распознавание во время ответа: событие speechstart позволяет
  // оборвать C-3PO сразу, как только пользователь начинает говорить.
  shouldListen = resumeListening;
  if (!resumeListening) recognition?.stop();
  voiceState.value = "speaking";

  const restoreVoiceState = () => {
    stopPrivateVoiceSpeech = null;
    assistantSpeechStartedAt = 0;
    bargeInStartedAt = 0;
    privateVoicePlaying = false;
    const canResume = resumeListening && microphonePermission.value === "granted" && !silentMode.value;
    voiceState.value = canResume ? "listening" : (silentMode.value ? "paused" : "idle");
    if (canResume) startRecognition();
  };
  let finishPlayback = () => {};
  const playbackFinished = new Promise<void>((resolve) => {
    finishPlayback = resolve;
  });
  privateVoiceAudio.onended = () => {
    restoreVoiceState();
    finishPlayback();
  };
  privateVoiceAudio.onerror = () => {
    restoreVoiceState();
    finishPlayback();
  };
  stopPrivateVoiceSpeech = () => {
    privateVoiceAudio?.pause();
    restoreVoiceState();
    finishPlayback();
  };
  try {
    await writeLog("info", "voice.private-pack", `Запуск локальной фразы ${phrase.id}`, {
      category,
      file: phrase.file,
    });
    await privateVoiceAudio.play();
    assistantSpeechStartedAt = performance.now();
    if (waitForCompletion) await playbackFinished;
    return true;
  } catch (error) {
    restoreVoiceState();
    finishPlayback();
    await writeLog("warn", "voice.private-pack", `Не удалось воспроизвести ${phrase.id}: ${String(error)}`);
    return false;
  }
}

async function loadLlmStatus() {
  if (!("__TAURI_INTERNALS__" in window)) {
    llmStatus.value = {
      state: "model-missing",
      engineFound: true,
      enginePath: "Браузерный предпросмотр",
      modelDirectory: "Доступно только в приложении Tauri",
      selectedModel: null,
      models: [],
      discoveredModels: 0,
      message: "В браузерном режиме показано демонстрационное состояние локальной модели.",
    };
    return;
  }

  llmLoading.value = true;
  try {
    llmStatus.value = await invoke<LlmStatus>("get_llm_status");
    const installedNames = (llmStatus.value.models ?? []).map((model) => model.name);
    if (modelPreference.value !== "auto" && !installedNames.includes(modelPreference.value)) {
      modelPreference.value = "auto";
      window.localStorage.setItem("c3po.model-preference", "auto");
    }
    llmRuntime.value = await invoke<LlmRuntimeStatus>("get_llm_runtime_status");
  } catch (error) {
    const details = errorDetails(error);
    await writeLog("error", "llm.status", `${details.name}: ${details.message}`);
  } finally {
    llmLoading.value = false;
  }
}

async function installRecommendedModels() {
  if (!("__TAURI_INTERNALS__" in window) || llmLoading.value) return;
  llmLoading.value = true;
  modelInstallMessage.value = "Скачиваю три модели (около 5,7 ГБ). Не закрывайте C-3PO…";
  try {
    const report = await invoke<ModelInstallReport>("install_recommended_models");
    modelInstallMessage.value = report.installed.length
      ? `${report.message}. Загружено: ${formatBytes(report.downloadedBytes)}.`
      : "Все рекомендованные модели уже установлены.";
    await writeLog("info", "llm.install", report.message, {
      installed: report.installed,
      skipped: report.skipped,
      downloadedBytes: report.downloadedBytes,
    });
  } catch (error) {
    const details = errorDetails(error);
    modelInstallMessage.value = `Установка не завершена: ${details.message}`;
    await writeLog("error", "llm.install", `${details.name}: ${details.message}`);
  } finally {
    llmLoading.value = false;
    await loadLlmStatus();
  }
}

async function waitForLocalModel() {
  // Не оставляем интерфейс в бесконечном ожидании, если llama-server не смог
  // запуститься (например, порт уже занят старым процессом).
  for (let attempt = 0; attempt < 60; attempt += 1) {
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
  await playPrivateVoice("shutdown");
  void playR2D2Signal("shutdown");
  await refreshProcessMetrics();
}

type R2D2Mood = "thinking" | "success" | "error" | "shutdown";

async function playR2D2Signal(mood: R2D2Mood) {
  if (silentMode.value || chatOpen.value) return;
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

async function askLocalModel(prompt: string, showInConversation = true, toolContext: string | null = null) {
  const consultationStartedAt = performance.now();
  isWorking.value = true;
  isConsultingR2.value = true;
  voiceState.value = "thinking";
  const activeDroid = selectedDroidName.value === "Автовыбор" ? "R2-D2" : selectedDroidName.value;
  lastCommand.value = `Минутку, я советуюсь с ${activeDroid}…`;
  try {
    // Даём Vue отрисовать R2-D2 до запуска тяжёлого локального запроса.
    await nextTick();
    await playR2D2Signal("thinking");
    if (!llmRuntime.value?.ready && !(await startLocalModel())) {
      throw new Error("Локальная модель не запустилась");
    }
    const now = new Date();
    const trustedTimeContext = [
      `Системная дата и время: ${new Intl.DateTimeFormat(selectedCommandLanguage.value.speechCode, {
        dateStyle: "full",
        timeStyle: "medium",
      }).format(now)}`,
      `ISO-дата: ${now.toISOString().slice(0, 10)}`,
      `Часовой пояс: ${Intl.DateTimeFormat().resolvedOptions().timeZone}`,
    ].join(". ");
    const combinedToolContext = toolContext
      ? `${trustedTimeContext}\n${toolContext}`
      : trustedTimeContext;
    const reply = await invoke<LocalReply>("generate_local_reply", {
      prompt,
      language: selectedCommandLanguage.value.modelName,
      userName: currentProfile.value?.name ?? null,
      preferredModel: modelPreference.value === "auto" ? null : modelPreference.value,
      toolContext: combinedToolContext,
    });
    llmTestReply.value = reply.text;
    lastCommand.value = reply.text;
    await playR2D2Signal("success");
    if (showInConversation) addMessage("assistant", reply.text);
    speak(reply.text);
    return reply.text;
  } catch (error) {
    await playR2D2Signal("error");
    await playPrivateVoice("error");
    const details = errorDetails(error);
    const message = `Локальная модель не ответила: ${details.message}`;
    llmTestReply.value = message;
    if (showInConversation) addMessage("assistant", message);
    voiceState.value = "error";
    await writeLog("error", "llm.generate", `${details.name}: ${details.message}`);
    return "";
  } finally {
    // Даже быстрый ответ или ошибка не должны превращать появление R2-D2 в
    // незаметную вспышку. Сначала гарантируем минимальное время показа, затем
    // оставляем сцену рядом с уже появившимся ответом ещё на две секунды.
    // Так пользователь видит именно передачу ответа от R2-D2 к C-3PO.
    if (chatOpen.value) {
      const remaining = 1_500 - (performance.now() - consultationStartedAt);
      if (remaining > 0) await new Promise((resolve) => window.setTimeout(resolve, remaining));
      await scrollChat("smooth");
      await new Promise((resolve) => window.setTimeout(resolve, 2_000));
    }
    isConsultingR2.value = false;
    isWorking.value = false;
    // Если синтез речи недоступен или включён тихий режим, speak() не меняет
    // состояние. Не оставляем экран навечно в режиме «думаю».
    if (voiceState.value === "thinking") {
      voiceState.value = microphoneActive.value ? "listening" : "idle";
    }
  }
}

function selectDroidModel(modelName: string) {
  modelPreference.value = modelName;
  window.localStorage.setItem("c3po.model-preference", modelName);
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

async function scrollChat(behavior: ScrollBehavior = "smooth") {
  await nextTick();
  conversation.value?.scrollTo({ top: conversation.value.scrollHeight, behavior });
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
  // Не позволяем микрофону принять голос самого C-3PO за новую команду.
  if (privateVoicePlaying) return;
  // Системные распознаватели записывают имя по-разному, поэтому принимаем
  // латинское C-3PO и наиболее частые русские транскрипции.
  const wakeWord = /^\s*(?:c\s*[- ]?3\s*[- ]?p\s*[- ]?o\b|c3po\b|с\s*[- ]?3\s*[- ]?п\s*[- ]?о\b|си\s*[- ]?три\s*[- ]?пи\s*[- ]?о|ситрипио\b)/iu;
  const wakeMatch = text.match(wakeWord);

  if (!wakeMatch) {
    // Пока окно скрыто, обычную речь игнорируем: C-3PO просыпается только
    // после своего имени и не превращает фоновые разговоры в команды.
    if (document.visibilityState === "visible") processVoiceCommand(text);
    return;
  }

  const command = text.replace(wakeWord, "").replace(/^[,.:;!?\s-]+/u, "").trim();
  if (command) {
    // Полная команда с именем выполняется в фоне. Окно не прыгает поверх
    // текущей работы пользователя, а результат приходит уведомлением macOS.
    const isBackgroundCommand = document.visibilityState !== "visible";
    if (isBackgroundCommand) void notifyFromBackground("Слышу вас. Минутку, я советуюсь с R2-D2…");
    processVoiceCommand(command, isBackgroundCommand);
  } else {
    if ("__TAURI_INTERNALS__" in window) await invoke("show_main_window");
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
    if (voiceState.value !== "speaking") voiceState.value = "listening";
  };
  recognition.onspeechstart = () => {
    // Web Speech уже отличил человеческую речь от обычного фонового шума.
    // Короткая защита нужна только от звука включения самого динамика.
    if (
      voiceState.value === "speaking"
      && assistantSpeechStartedAt > 0
      && performance.now() - assistantSpeechStartedAt > 250
    ) {
      interruptAssistantSpeech("speech-recognition");
    }
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

    const recognizedText = (interimText || finalText).trim();
    // Некоторые WebView не присылают speechstart, но уже отдают промежуточный
    // текст. Это второй надёжный сигнал, что человек заговорил поверх ответа.
    if (privateVoicePlaying && recognizedText) {
      if (assistantSpeechStartedAt > 0 && performance.now() - assistantSpeechStartedAt > 250) {
        interruptAssistantSpeech("recognized-text");
      } else {
        return;
      }
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
  const samples = new Uint8Array(512);
  let noiseFloor = 0.012;
  analyser.fftSize = 512;
  source.connect(analyser);

  const update = () => {
    analyser.getByteTimeDomainData(samples);
    const meanSquare = samples.reduce((sum, value) => {
      const normalized = (value - 128) / 128;
      return sum + normalized * normalized;
    }, 0) / samples.length;
    const rms = Math.sqrt(meanSquare);
    audioLevel.value = Math.min(1, rms * 8);

    // В паузах запоминаем обычный шум комнаты. Поэтому тихая речь рядом с
    // микрофоном срабатывает, а постоянный гул вентилятора — нет.
    if (voiceState.value !== "speaking") {
      noiseFloor = noiseFloor * 0.96 + rms * 0.04;
    }
    const speechThreshold = Math.max(0.028, noiseFloor * 2.35);

    // Barge-in: echoCancellation подавляет собственный голос C-3PO, а
    // короткий устойчивый голосовой пик служит резервом для WebView, который
    // не поддерживает событие speechstart.
    if (
      voiceState.value === "speaking"
      && assistantSpeechStartedAt > 0
      && performance.now() - assistantSpeechStartedAt > 300
      && rms >= speechThreshold
    ) {
      if (!bargeInStartedAt) bargeInStartedAt = performance.now();
      else if (performance.now() - bargeInStartedAt >= 90) interruptAssistantSpeech("audio-meter");
    } else {
      bargeInStartedAt = 0;
    }
    meterFrame = window.requestAnimationFrame(update);
  };
  update();
}

function interruptAssistantSpeech(
  source: "speech-recognition" | "recognized-text" | "audio-meter" | "chat-opened" = "audio-meter",
) {
  if (voiceState.value !== "speaking") return;
  speechRunId += 1;
  window.speechSynthesis?.cancel();
  stopGeneratedSpeech?.();
  stopPrivateVoiceSpeech?.();
  privateVoiceAudio?.pause();
  privateVoicePlaying = false;
  assistantSpeechStartedAt = 0;
  bargeInStartedAt = 0;
  liveTranscript.value = "";
  if (microphonePermission.value === "granted" && recognitionAvailable.value) {
    voiceState.value = "listening";
    shouldListen = true;
    startRecognition();
  } else {
    voiceState.value = "idle";
  }
  const message = source === "chat-opened"
    ? "Озвучка остановлена: пользователь открыл текстовый диалог"
    : "Озвучка остановлена новой речью пользователя";
  void writeLog("info", "voice.barge-in", message, { source });
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

  if (silentMode.value) {
    microphonePermission.value = (await hasRememberedMicrophoneApproval()) ? "granted" : "prompt";
    onboardingOpen.value = false;
    voiceState.value = "paused";
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

async function waitForTtsServer() {
  for (let attempt = 0; attempt < 80; attempt += 1) {
    ttsStatus.value = await invoke<TtsStatus>("get_tts_status");
    if (ttsStatus.value.state === "running") return true;
    if (ttsStatus.value.state === "runtime-missing" || ttsStatus.value.state === "reference-missing") return false;
    await new Promise((resolve) => window.setTimeout(resolve, 250));
  }
  return false;
}

async function ensureTtsServer() {
  if (!("__TAURI_INTERNALS__" in window)) return false;
  try {
    ttsStatus.value = await invoke<TtsStatus>("get_tts_status");
    if (ttsStatus.value.state === "running") return true;
    if (!ttsStatus.value.runtimeFound || !ttsStatus.value.referenceFound) return false;
    ttsStatus.value = await invoke<TtsStatus>("start_tts");
    return await waitForTtsServer();
  } catch (error) {
    const details = errorDetails(error);
    await writeLog("error", "tts.start", `${details.name}: ${details.message}`);
    return false;
  }
}

function prepareSpeechText(text: string) {
  const codeNotice = commandLanguage.value === "ru"
    ? "Фрагмент кода показан в истории диалога."
    : "The code sample is available in the conversation history.";
  let spoken = text
    // Голос не должен зачитывать Markdown, URL и служебные символы модели.
    .replace(/```[\s\S]*?```/gu, ` ${codeNotice} `)
    .replace(/\[([^\]]+)\]\([^)]+\)/gu, "$1")
    .replace(/https?:\/\/\S+/giu, "")
    .replace(/^\s{0,3}#{1,6}\s*/gmu, "")
    .replace(/^\s*(?:[-*•]|\d+[.)])\s+/gmu, "")
    .replace(/[*_~`>|]/gu, "")
    .replace(/\s*\n+\s*/gu, ". ")
    .replace(/\s+/gu, " ")
    .trim();

  // Названия дроидов и технические сокращения иначе читаются как случайный
  // набор латинских букв. Русская транскрипция делает речь предсказуемой.
  if (commandLanguage.value === "ru") {
    spoken = spoken
      .replace(/\bC\s*[- ]?3\s*P\s*[- ]?O\b/giu, "Си-три-пи-о")
      .replace(/\bR\s*2\s*[- ]?D\s*2\b/giu, "Эр-два-Дэ-два")
      .replace(/\bRAM\b/giu, "оперативная память")
      .replace(/\bmacOS\b/giu, "мак-о-эс")
      .replace(/\bWindows\b/giu, "Виндоус");
  }
  return spoken.replace(/(?:\.\s*){2,}/gu, ". ").trim();
}

function splitSpeechText(text: string, maxCharacters = 140) {
  const chunks: string[] = [];
  const sentences = text.trim().match(/[^.!?。！？]+[.!?。！？]*/gu) ?? [text.trim()];
  let paragraph = "";

  const flushParagraph = () => {
    if (paragraph) chunks.push(paragraph);
    paragraph = "";
  };

  for (const sentence of sentences.map((value) => value.trim()).filter(Boolean)) {
    if (sentence.length <= maxCharacters) {
      const combined = paragraph ? `${paragraph} ${sentence}` : sentence;
      if (combined.length <= maxCharacters) {
        paragraph = combined;
      } else {
        flushParagraph();
        paragraph = sentence;
      }
      continue;
    }

    flushParagraph();
    let part = "";
    for (const word of sentence.split(/\s+/u)) {
      const candidate = part ? `${part} ${word}` : word;
      if (candidate.length > maxCharacters && part) {
        chunks.push(part);
        part = word;
      } else {
        part = candidate;
      }
    }
    if (part) chunks.push(part);
  }
  flushParagraph();
  return chunks;
}

async function synthesizeSpeechChunk(text: string) {
  if (!ttsStatus.value) throw new Error("Локальный голосовой процесс недоступен");
  const response = await fetch(`${ttsStatus.value.address}/synthesize`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ text, language: commandLanguage.value }),
  });
  if (!response.ok) {
    const details = await response.text();
    throw new Error(`Голосовой движок вернул ${response.status}: ${details.slice(0, 500)}`);
  }
  return response.blob();
}

async function playGeneratedSpeech(blob: Blob, runId: number) {
  if (runId !== speechRunId) return;
  generatedVoiceAudio?.pause();
  if (generatedVoiceUrl) URL.revokeObjectURL(generatedVoiceUrl);
  generatedVoiceUrl = URL.createObjectURL(blob);
  generatedVoiceAudio = new Audio(generatedVoiceUrl);
  generatedVoiceAudio.preload = "auto";
  generatedVoiceAudio.volume = 0.92;

  await new Promise<void>((resolve, reject) => {
    let finished = false;
    const finish = (error?: unknown) => {
      if (finished) return;
      finished = true;
      stopGeneratedSpeech = null;
      if (error) reject(error);
      else resolve();
    };
    stopGeneratedSpeech = () => {
      generatedVoiceAudio?.pause();
      finish();
    };
    generatedVoiceAudio!.onended = () => finish();
    generatedVoiceAudio!.onerror = () => finish(new Error("Не удалось воспроизвести сгенерированный звук"));
    void generatedVoiceAudio!.play().catch(finish);
  });
}

async function playSystemSpeech(text: string, runId: number) {
  if (!("speechSynthesis" in window) || typeof SpeechSynthesisUtterance === "undefined") {
    throw new Error("Системная озвучка недоступна");
  }

  await new Promise<void>((resolve, reject) => {
    let finished = false;
    const utterance = new SpeechSynthesisUtterance(text);
    utterance.lang = selectedCommandLanguage.value.speechCode;
    utterance.rate = 0.96;
    utterance.pitch = 0.92;
    const languagePrefix = utterance.lang.toLowerCase().split("-")[0];
    const matchingVoices = window.speechSynthesis.getVoices().filter(
      (voice) => voice.lang.toLowerCase().startsWith(languagePrefix),
    );
    // Системный список часто начинается с женского голоса. Для образа C-3PO
    // сначала ищем распространённые мужские голоса macOS и Windows.
    const preferredMaleNames = /alex|aleks|алекс|yuri|юрий|maxim|максим|pavel|павел|dmitri|дмитрий|mikhail|михаил/i;
    utterance.voice = matchingVoices.find((voice) => preferredMaleNames.test(voice.name))
      ?? matchingVoices[0]
      ?? null;

    const finish = (error?: unknown) => {
      if (finished) return;
      finished = true;
      stopGeneratedSpeech = null;
      if (error) reject(error);
      else resolve();
    };
    stopGeneratedSpeech = () => {
      window.speechSynthesis.cancel();
      finish();
    };
    utterance.onend = () => finish();
    utterance.onerror = (event) => finish(new Error(`Системная озвучка: ${event.error}`));
    if (runId !== speechRunId) return finish();
    window.speechSynthesis.cancel();
    window.speechSynthesis.speak(utterance);
  });
}

async function speak(text: string) {
  // Открытая панель означает, что пользователь выбрал текстовый сценарий.
  // Не дублируем видимый ответ голосом и не мешаем читать историю.
  if (silentMode.value || chatOpen.value || !text.trim()) return;
  const spokenText = prepareSpeechText(text).slice(0, 1_000);
  if (!spokenText) return;
  stopGeneratedSpeech?.();
  const runId = ++speechRunId;
  const resumeListening = shouldListen;
  privateVoicePlaying = true;
  // Слушаем параллельно с озвучкой. echoCancellation убирает голос из
  // динамиков, а речь пользователя немедленно запускает barge-in.
  shouldListen = resumeListening;
  if (!resumeListening) recognition?.stop();
  voiceState.value = "speaking";
  assistantSpeechStartedAt = performance.now();

  const restoreVoiceState = () => {
    assistantSpeechStartedAt = 0;
    bargeInStartedAt = 0;
    privateVoicePlaying = false;
    const canResume = resumeListening && microphonePermission.value === "granted" && !silentMode.value;
    voiceState.value = canResume ? "listening" : (silentMode.value ? "paused" : "idle");
    if (canResume) startRecognition();
  };

  try {
    // Быстрый режим использует уже установленный в ОС движок и поэтому
    // подходит даже слабым компьютерам. В «Авто» личные записанные реплики
    // сохраняются, а произвольный длинный ответ начинается без ожидания ИИ-TTS.
    // При раннем ответе состояние могло ещё не успеть загрузиться. Повторная
    // нативная проверка не даёт режиму «Авто» внезапно перейти на женский
    // системный голос, если личный профиль уже установлен.
    if (voiceMode.value === "adaptive" && "__TAURI_INTERNALS__" in window
      && !(ttsStatus.value?.runtimeFound && ttsStatus.value.referenceFound)) {
      ttsStatus.value = await invoke<TtsStatus>("get_tts_status").catch(() => ttsStatus.value);
    }
    const personalVoiceReady = Boolean(ttsStatus.value?.runtimeFound && ttsStatus.value.referenceFound);
    const useFastSystemVoice = voiceMode.value === "instant"
      || (voiceMode.value === "adaptive" && !personalVoiceReady);
    if (useFastSystemVoice && "speechSynthesis" in window) {
      await writeLog("info", "tts.system", "Быстрая озвучка ответа системным голосом", {
        characters: spokenText.length,
        language: commandLanguage.value,
        mode: voiceMode.value,
      });
      await playSystemSpeech(spokenText, runId);
      selectedVoiceName.value = voiceMode.value === "adaptive"
        ? "Авто · системный голос (личный профиль не установлен)"
        : "Быстрый системный голос";
      if (runId === speechRunId) restoreVoiceState();
      return;
    }

    if (!(await ensureTtsServer()) || !ttsStatus.value) {
      throw new Error(ttsStatus.value?.message ?? "Локальный голосовой компонент не установлен");
    }
    const chunks = splitSpeechText(spokenText);
    await writeLog("info", "tts.synthesize", "Начата локальная озвучка полного ответа", {
      characters: spokenText.length,
      language: commandLanguage.value,
      chunks: chunks.length,
    });
    let pendingChunk = synthesizeSpeechChunk(chunks[0]);
    for (let index = 0; index < chunks.length; index += 1) {
      const audio = await pendingChunk;
      if (runId !== speechRunId) return;
      // Следующий фрагмент генерируется уже во время воспроизведения текущего.
      // Это скрывает большую часть вычислительной задержки длинного ответа.
      const nextChunk = index + 1 < chunks.length
        ? synthesizeSpeechChunk(chunks[index + 1])
        : null;
      await playGeneratedSpeech(audio, runId);
      if (nextChunk) pendingChunk = nextChunk;
    }
    synthesisAvailable.value = true;
    selectedVoiceName.value = "Локальный голос пользователя";
    if (runId === speechRunId) restoreVoiceState();
  } catch (error) {
    if (runId === speechRunId) restoreVoiceState();
    const details = errorDetails(error);
    await writeLog("error", "tts.synthesize", `${details.name}: ${details.message}`);
    // Ответ уже находится в чате. Не подменяем его случайной заготовленной
    // фразой: при проблеме с TTS пользователь всё равно получает точный текст.
  }
}

async function testVoice() {
  // Проверяем тот же тракт, которым озвучиваются реальные ответы, а не
  // заранее нарезанный каталог ситуационных фраз.
  const russianSamples = [
    "Здравствуйте. Я Си-три-пи-о. Проверяю тембр, темп и естественность своей речи.",
    "Google Chrome найден среди установленных приложений. Я готов открыть его по вашей команде.",
    "Сегодня я говорю спокойнее, делаю логичные паузы и не зачитываю служебную разметку.",
  ];
  const phrase = commandLanguage.value === "ru"
    ? russianSamples[voiceDemoIndex % russianSamples.length]
    : commandText("greeting");
  voiceDemoIndex += 1;
  lastCommand.value = phrase;
  addMessage("assistant", phrase);
  speak(phrase);
}

function proposeSystemInfo() {
  pendingAction.value = { kind: "system-info" };
  voiceState.value = "idle";
  const response = `${moviePhrase("riskyAction")} Скажите «выполнить» или нажмите кнопку подтверждения.`;
  addMessage("assistant", response);
  void playPrivateVoice("warning").then((played) => {
    if (!played) speak(response);
  });
}

function cancelAction() {
  pendingAction.value = null;
  awaitingTelegramMessageContact.value = "";
  const response = assistantPhrase("actionCancelled");
  addMessage("assistant", response);
  speak(response);
}

async function approveAction() {
  const action = pendingAction.value;
  if (!action) return;
  isWorking.value = true;
  voiceState.value = "thinking";

  try {
    if (action.kind === "telegram-message") {
      if (!("__TAURI_INTERNALS__" in window)) throw new Error("Отправка доступна только в приложении C-3PO");
      const result = await invoke<NativeActionResult>("send_telegram_message", {
        contact: action.contact,
        message: action.message,
      });
      pendingAction.value = null;
      addMessage("assistant", result.message);
      lastCommand.value = result.message;
      await writeLog("info", "executor.telegram", "Сообщение отправлено после явного подтверждения", {
        contactLength: action.contact.length,
        messageLength: action.message.length,
      });
      speak(result.message);
      return;
    }

    const info = "__TAURI_INTERNALS__" in window
      ? await invoke<SystemInfo>("get_system_info")
      : environment.value?.system;

    if (!info) throw new Error("Сведения о системе пока недоступны");

    pendingAction.value = null;
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
    const details = errorDetails(error);
    const response = action.kind === "telegram-message"
      ? `Не удалось отправить сообщение. ${details.message}`
      : `${moviePhrase("criticalError")} ${details.message}`;
    addMessage("assistant", response);
    lastCommand.value = response;
    await writeLog("error", `executor.${action.kind}`, `${details.name}: ${details.message}`);
    speak(response);
  } finally {
    isWorking.value = false;
  }
}

function parseTelegramMessageCommand(text: string) {
  const namesTelegram = /(telegram|телеграмм?)/iu.test(text);
  const explicitlyNamesContact = /(напиши|отправь)(?:\s+сообщение)?\s+контакту/iu.test(text);
  if (!/(напиши|отправь)/iu.test(text) || (!namesTelegram && !explicitlyNamesContact)) return null;
  const full = text.match(/контакту\s+(.+?)\s+(?:сообщение|текст)\s+(.+)$/iu)
    ?? text.match(/контакту\s+([^,:]+?)\s*[:,]\s*(.+)$/iu);
  if (full) return { contact: full[1].trim(), message: full[2].trim() };

  const contact = text.match(/контакту\s+(.+?)(?:\s+и\s+(?:я\s+)?(?:диктую|продиктую)\s+сообщение)?$/iu)?.[1]?.trim();
  return contact ? { contact, message: "" } : null;
}

async function openKnownApplication(application: string) {
  try {
    if (!("__TAURI_INTERNALS__" in window)) throw new Error("Запуск программ доступен только в приложении C-3PO");
    const result = await invoke<NativeActionResult>("open_application", { application });
    addMessage("assistant", result.message);
    lastCommand.value = result.message;
    speak(result.message);
  } catch (error) {
    const details = errorDetails(error);
    const response = `Не удалось открыть «${application}»: ${details.message}`;
    addMessage("assistant", response);
    lastCommand.value = response;
    await writeLog("error", "executor.open-application", `${details.name}: ${details.message}`);
    speak(response);
  }
}

function arithmeticExpressionFrom(text: string) {
  const requested = text.match(/(?:посчитай|вычисли|сколько\s+будет|calculate)\s+(.+)$/iu)?.[1];
  if (!requested) return null;
  const normalized = requested
    .replace(/(\d),(\d)/gu, "$1.$2")
    .replace(/[×хx]/giu, "*")
    .replace(/[÷:]/gu, "/")
    .replace(/\b(?:плюс|plus)\b/giu, "+")
    .replace(/\b(?:минус|minus)\b/giu, "-")
    .replace(/\b(?:умножить\s+на|умножь\s+на|times)\b/giu, "*")
    .replace(/\b(?:разделить\s+на|раздели\s+на|divided\s+by)\b/giu, "/")
    .trim();
  return /^[\d+\-*/().\s]+$/u.test(normalized) ? normalized : null;
}

function evaluateArithmetic(expression: string) {
  let position = 0;
  const skipSpaces = () => { while (/\s/u.test(expression[position] ?? "")) position += 1; };
  const parseNumber = (): number => {
    skipSpaces();
    const start = position;
    while (/[\d.]/u.test(expression[position] ?? "")) position += 1;
    const value = Number(expression.slice(start, position));
    if (!Number.isFinite(value) || start === position) throw new Error("Ожидалось число");
    return value;
  };
  const parseFactor = (): number => {
    skipSpaces();
    if (expression[position] === "+") { position += 1; return parseFactor(); }
    if (expression[position] === "-") { position += 1; return -parseFactor(); }
    if (expression[position] === "(") {
      position += 1;
      const value = parseSum();
      skipSpaces();
      if (expression[position] !== ")") throw new Error("Не закрыта скобка");
      position += 1;
      return value;
    }
    return parseNumber();
  };
  const parseProduct = (): number => {
    let value = parseFactor();
    while (true) {
      skipSpaces();
      const operator = expression[position];
      if (operator !== "*" && operator !== "/") return value;
      position += 1;
      const operand = parseFactor();
      if (operator === "/" && operand === 0) throw new Error("На ноль делить нельзя");
      value = operator === "*" ? value * operand : value / operand;
    }
  };
  const parseSum = (): number => {
    let value = parseProduct();
    while (true) {
      skipSpaces();
      const operator = expression[position];
      if (operator !== "+" && operator !== "-") return value;
      position += 1;
      const operand = parseProduct();
      value = operator === "+" ? value + operand : value - operand;
    }
  };
  const result = parseSum();
  skipSpaces();
  if (position !== expression.length || !Number.isFinite(result)) throw new Error("Некорректное выражение");
  return result;
}

function answerCalculation(command: string) {
  const expression = arithmeticExpressionFrom(command);
  if (!expression) return false;
  try {
    const result = evaluateArithmetic(expression);
    const formatted = new Intl.NumberFormat(selectedCommandLanguage.value.speechCode, {
      maximumFractionDigits: 10,
    }).format(result);
    const response = `Результат: ${formatted}.`;
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
  } catch (error) {
    const response = `Не удалось посчитать: ${errorDetails(error).message}.`;
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
  }
  return true;
}

function parseOpenApplicationCommand(text: string) {
  const requested = text.match(/^(?:открой|запусти|open|launch)\s+(?:приложение\s+|app\s+)?(.+?)\s*$/iu)?.[1];
  if (!requested) return null;
  // Составную фразу разбираем на цель и следующее действие. Без этого весь
  // хвост ошибочно становился названием приложения.
  const parts = requested.split(/\s+и\s+(?=(?:посчитай|вычисли|сколько\s+будет|calculate)\b)/iu, 2);
  return { application: parts[0].trim(), followUp: parts[1]?.trim() || "" };
}

async function executeOpenApplicationCommand(application: string, followUp: string) {
  await openKnownApplication(application);
  if (followUp) answerCalculation(followUp);
}

async function answerWeatherQuestion(question: string, notifyInBackground: boolean) {
  isWorking.value = true;
  voiceState.value = "thinking";
  lastCommand.value = "Получаю актуальную погоду в фоновом режиме…";
  try {
    if (!("__TAURI_INTERNALS__" in window)) {
      throw new Error("Фоновая погода доступна только в приложении C-3PO");
    }
    const report = await invoke<WeatherReport>("get_current_weather");
    await writeLog("info", "tool.weather", "Получена актуальная сводка в фоновом режиме", {
      source: report.source,
      characters: report.summary.length,
    });
    const toolContext = `Источник: ${report.source}. Актуальная сводка: ${report.summary}`;
    const reply = await askLocalModel(question, true, toolContext);
    if (reply) {
      if (notifyInBackground) void notifyFromBackground(reply);
      return;
    }

    // Даже если локальная LLM временно не загрузилась, показываем реальные
    // данные инструмента, а не придумываем погоду.
    const fallback = `${report.summary}. Источник: ${report.source}.`;
    addMessage("assistant", fallback);
    lastCommand.value = fallback;
    speak(fallback);
    if (notifyInBackground) void notifyFromBackground(fallback);
    isWorking.value = false;
  } catch (error) {
    const details = errorDetails(error);
    const response = `Не удалось получить актуальную погоду: ${details.message}`;
    addMessage("assistant", response);
    lastCommand.value = response;
    voiceState.value = microphoneActive.value ? "listening" : "error";
    await writeLog("error", "tool.weather", `${details.name}: ${details.message}`);
    speak(response);
    if (notifyInBackground) void notifyFromBackground(response);
    isWorking.value = false;
  }
}

function processVoiceCommand(text: string, notifyInBackground = false, toolContext: string | null = null) {
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

  if (awaitingTelegramMessageContact.value) {
    if (/^(отмена|отменить|стоп|cancel)$/iu.test(normalized.trim())) {
      cancelAction();
      return;
    }
    const contact = awaitingTelegramMessageContact.value;
    awaitingTelegramMessageContact.value = "";
    pendingAction.value = { kind: "telegram-message", contact, message: text.trim() };
    chatOpen.value = true;
    const response = `Я подготовил сообщение для контакта «${contact}»: «${text.trim()}». Для отправки скажите «отправить сообщение» или нажмите кнопку.`;
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
    return;
  }

  if (/(другой пользователь|новый пользователь|another user|new user|otro usuario|neuer benutzer|nouvel utilisateur|nuovo utente|novo usuário|新用户|別のユーザー|مستخدم جديد)/i.test(normalized)) {
    startNewSpeaker();
    return;
  }

  if (pendingAction.value?.kind === "telegram-message" && /^(отправить сообщение|подтверждаю отправку)$/iu.test(normalized.trim())) {
    void approveAction();
    return;
  }
  if (pendingAction.value?.kind === "telegram-message" && /(выполн|разреш|подтверж|да)/.test(normalized)) {
    const response = "Для защиты от случайной отправки скажите точно: «отправить сообщение», либо нажмите кнопку «Отправить».";
    addMessage("assistant", response);
    speak(response);
    return;
  }
  if (pendingAction.value?.kind === "system-info" && /(выполн|разреш|подтверж|да)/.test(normalized)) {
    void approveAction();
    return;
  }
  if (pendingAction.value && /(отмен|нет|стоп)/.test(normalized)) {
    cancelAction();
    return;
  }
  const telegramCommand = parseTelegramMessageCommand(text);
  if (telegramCommand) {
    if (!telegramCommand.message) {
      awaitingTelegramMessageContact.value = telegramCommand.contact;
      const response = `Telegram откроется после подтверждения отправки. Продиктуйте сообщение для контакта «${telegramCommand.contact}».`;
      addMessage("assistant", response);
      lastCommand.value = response;
      speak(response);
      return;
    }
    pendingAction.value = { kind: "telegram-message", ...telegramCommand };
    chatOpen.value = true;
    if (notifyInBackground && "__TAURI_INTERNALS__" in window) void invoke("show_main_window");
    const response = `Проверьте сообщение для контакта «${telegramCommand.contact}»: «${telegramCommand.message}». Скажите «отправить сообщение» или нажмите кнопку.`;
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
    return;
  }
  if (normalized.includes(localizedSystemCommand) || /(систем|компьютер|операцион|памят|процессор)/.test(normalized)) {
    proposeSystemInfo();
    if (notifyInBackground) void notifyFromBackground("Для показа системной информации нужно ваше подтверждение. Откройте C-3PO.");
    return;
  }
  if (/(погод|weather|tiempo|wetter|m[ée]t[ée]o|tempo|天气|天気|طقس)/iu.test(normalized)) {
    void answerWeatherQuestion(text, notifyInBackground);
    return;
  }
  if (/(какой сегодня день|какая сегодня дата|сегодняшн.*(?:день|дата)|который час|сколько времени|what day is it|what(?:'s| is) the date|what time is it)/iu.test(normalized)) {
    const now = new Date();
    const response = `Сегодня ${new Intl.DateTimeFormat(selectedCommandLanguage.value.speechCode, {
      dateStyle: "full",
      timeStyle: /час|врем|time/iu.test(normalized) ? "short" : undefined,
    }).format(now)}. Часовой пояс: ${Intl.DateTimeFormat().resolvedOptions().timeZone}.`;
    addMessage("assistant", response);
    lastCommand.value = response;
    speak(response);
    if (notifyInBackground) void notifyFromBackground(response);
    return;
  }
  if (normalized.includes(localizedChatCommand) || /(открой|покажи).*(чат|диалог)/.test(normalized)) {
    if (notifyInBackground && "__TAURI_INTERNALS__" in window) void invoke("show_main_window");
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
  const openApplication = parseOpenApplicationCommand(text);
  if (openApplication) {
    void executeOpenApplicationCommand(openApplication.application, openApplication.followUp);
    return;
  }
  if (answerCalculation(text)) {
    return;
  }

  if (llmStatus.value?.state === "ready") {
    void askLocalModel(text, true, toolContext).then((reply) => {
      if (notifyInBackground && reply) void notifyFromBackground(reply);
    });
    return;
  }

  const response = `${moviePhrase("unknownBehavior")} Сейчас доступна информация о системе.`;
  addMessage("assistant", response);
  speak(response);
  if (notifyInBackground) void notifyFromBackground(response);
}

async function selectFilesForAnalysis(event: Event) {
  const inputElement = event.target as HTMLInputElement;
  const files = Array.from(inputElement.files ?? []).slice(0, 10);
  inputElement.value = "";
  const allowedExtensions = new Set([
    "txt", "md", "json", "csv", "log", "rs", "toml", "ts", "tsx", "js", "jsx", "vue", "css", "html", "py", "php", "sql", "yaml", "yml",
  ]);
  const accepted = files.filter((file) => {
    const extension = file.name.split(".").pop()?.toLowerCase() ?? "";
    return allowedExtensions.has(extension) && file.size <= 1_000_000;
  });
  if (!accepted.length) {
    lastCommand.value = "Выберите текстовые файлы размером до 1 МБ каждый.";
    return;
  }

  let remainingCharacters = 1_700;
  const fragments: string[] = [];
  const names: string[] = [];
  for (const file of accepted) {
    if (remainingCharacters <= 0) break;
    const text = (await file.text()).replace(/\0/g, "").trim();
    const excerpt = text.slice(0, remainingCharacters);
    fragments.push(`Файл: ${file.webkitRelativePath || file.name}\n${excerpt}`);
    names.push(file.webkitRelativePath || file.name);
    remainingCharacters -= excerpt.length;
  }
  selectedFileNames.value = names;
  selectedFileContext.value = `Содержимое файлов, которые пользователь явно выбрал только для чтения:\n\n${fragments.join("\n\n")}`;
  lastCommand.value = `Подготовлено к локальному анализу: ${names.join(", ")}`;
}

function clearSelectedFiles() {
  selectedFileNames.value = [];
  selectedFileContext.value = "";
}

function submitMessage() {
  const text = input.value.trim();
  if (!text || isWorking.value) return;
  input.value = "";
  const fileContext = selectedFileContext.value || null;
  clearSelectedFiles();
  processVoiceCommand(text, false, fileContext);
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

function selectTheme(theme: ThemePreference) {
  themePreference.value = theme;
  applyTheme();
}

function selectVoiceMode(mode: VoiceMode) {
  voiceMode.value = mode;
  window.localStorage.setItem("c3po.voice-mode", mode);
  const personalVoiceReady = Boolean(ttsStatus.value?.runtimeFound && ttsStatus.value.referenceFound);
  selectedVoiceName.value = mode === "personal" || (mode === "adaptive" && personalVoiceReady)
    ? "Локальный голос пользователя"
    : (mode === "instant" ? "Быстрый системный голос" : "Авто · системный голос");
  // Смена режима также должна немедленно остановить предыдущую озвучку.
  speechRunId += 1;
  window.speechSynthesis?.cancel();
  stopGeneratedSpeech?.();
  stopPrivateVoiceSpeech?.();
  if (mode === "personal" || (mode === "adaptive" && personalVoiceReady)) void ensureTtsServer();
}

async function toggleSilentMode() {
  silentMode.value = !silentMode.value;
  window.localStorage.setItem("c3po.silent-mode", String(silentMode.value));
  if (silentMode.value) {
    onboardingOpen.value = false;
    pauseMicrophone();
    window.speechSynthesis?.cancel();
    privateVoiceAudio?.pause();
    speechRunId += 1;
    stopGeneratedSpeech?.();
    stopPrivateVoiceSpeech?.();
    return;
  }
  if (await hasRememberedMicrophoneApproval()) await requestMicrophone();
  else onboardingOpen.value = true;
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
  await ensureNotificationPermission();
  await loadPrivateVoiceCatalog();
  await loadLlmStatus();
  if ("__TAURI_INTERNALS__" in window) {
    ttsStatus.value = await invoke<TtsStatus>("get_tts_status").catch(() => null);
    if (ttsStatus.value?.runtimeFound && ttsStatus.value.referenceFound) {
      synthesisAvailable.value = true;
      selectedVoiceName.value = "Локальный голос пользователя";
      if (voiceMode.value !== "instant") {
        // В «Авто» установленный личный профиль важнее системного голоса.
        // Отказаться от тяжёлой модели можно явным выбором «Быстрый».
        void ensureTtsServer();
      }
    }
  }
  configureRecognition();
  if (privateVoiceCatalog.value && !(ttsStatus.value?.runtimeFound && ttsStatus.value.referenceFound)) {
    synthesisAvailable.value = true;
    selectedVoiceName.value = "Локальный пакет voice.mp3";
  }
  if (voiceMode.value === "instant") {
    selectedVoiceName.value = "Быстрый системный голос";
  } else if (voiceMode.value === "adaptive" && !(ttsStatus.value?.runtimeFound && ttsStatus.value.referenceFound)) {
    selectedVoiceName.value = "Авто · системный голос (личный профиль не установлен)";
  }
  await refreshProcessMetrics();
  // Снимаем лёгкий замер каждые пять секунд. Базовое и пиковое значения
  // позволяют заметить устойчивый рост памяти во время долгой работы.
  memoryTimer = window.setInterval(refreshProcessMetrics, 5_000);
  // Если macOS уже запомнила разрешение, сначала подключаем анализатор уровня
  // микрофона. Благодаря этому даже стартовое приветствие можно немедленно
  // прервать своей речью. Само распознавание на время фразы приостанавливается
  // внутри playPrivateVoice, поэтому C-3PO не принимает свой голос за команду.
  await inspectMicrophonePermission();
  await playPrivateVoice("startup", true, true);
});

onBeforeUnmount(() => {
  window.clearInterval(memoryTimer);
  pauseMicrophone();
  window.speechSynthesis?.cancel();
  audioContext?.close();
  droidAudioContext?.close();
  privateVoiceAudio?.pause();
  speechRunId += 1;
  stopGeneratedSpeech?.();
  stopPrivateVoiceSpeech?.();
  if (generatedVoiceUrl) URL.revokeObjectURL(generatedVoiceUrl);
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
        <button class="ghost-button settings-button" type="button" :aria-label="settingsLabels[interfaceLanguage].settings" @click="settingsOpen = true">
          <span class="button-icon">⚙</span>
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
          <button type="button" @click="processVoiceCommand('Открой Telegram')">
            <span class="command-icon">✈</span>
            <span>
              <strong>«Открой Telegram»</strong>
              <small>Сообщения отправляются только после подтверждения</small>
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
          <h3>{{ pendingActionTitle }}</h3>
          <p>{{ pendingActionDescription }}</p>
        </div>
        <div class="approval-actions">
          <button type="button" @click="cancelAction">Отмена</button>
          <button class="approve-button" type="button" :disabled="isWorking" @click="approveAction">
            {{ isWorking ? "Выполняю…" : pendingActionButton }}
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
            <div class="conversation-browser-actions">
              <button
                v-if="conversationThreads.length"
                class="clear-conversations"
                type="button"
                @click="clearAllConversations"
              >{{ dialogLabels[interfaceLanguage].clear }}</button>
              <button type="button" @click="startNewConversation">＋ {{ dialogLabels[interfaceLanguage].newDialog }}</button>
            </div>
          </header>
          <p v-if="!conversationThreads.length" class="conversation-empty">
            {{ dialogLabels[interfaceLanguage].empty }}
          </p>
          <article
            v-for="thread in conversationThreads"
            :key="thread.id"
            class="conversation-item"
            :class="{ active: thread.id === activeConversationId }"
          >
            <button class="conversation-open" type="button" @click="openConversation(thread)">
              <span>◇</span>
              <span><strong>{{ thread.title }}</strong><small>{{ formatConversationDate(thread.updatedAt) }}</small></span>
              <b>→</b>
            </button>
            <button
              class="conversation-delete"
              type="button"
              :aria-label="dialogLabels[interfaceLanguage].deleteDialog"
              :title="dialogLabels[interfaceLanguage].deleteDialog"
              @click="deleteConversation(thread)"
            >×</button>
          </article>
        </section>

        <div v-else ref="conversation" class="drawer-messages">
          <div v-for="message in messages" :key="message.id" class="drawer-message" :class="message.role">
            <span v-if="message.role === 'assistant'" class="small-avatar">C</span>
            <p>{{ message.text }}</p>
          </div>
          <div v-if="isConsultingR2" class="drawer-message assistant r2-chat-message" aria-live="polite">
            <span class="small-avatar">C</span>
            <div class="r2-chat-bubble">
              <div class="r2-chat-visual" aria-hidden="true">
                <div class="r2-hologram">
                  <span class="r2-orbit orbit-a"></span>
                  <span class="r2-orbit orbit-b"></span>
                  <div class="r2-unit">
                    <div class="r2-dome"><i></i><b></b></div>
                    <div class="r2-body"><i></i><i></i><i></i></div>
                    <span class="r2-leg leg-left"></span>
                    <span class="r2-leg leg-right"></span>
                  </div>
                  <div class="r2-beam"></div>
                </div>
              </div>
              <div class="r2-chat-copy">
                <span>C-3PO ↔ R2-D2</span>
                <strong>R2-D2 подключён · идёт разговор с C-3PO</strong>
                <small>R2-D2: бип-буп · анализирую запрос и готовлю ответ</small>
                <div class="r2-data-line" aria-hidden="true"><i v-for="index in 12" :key="index"></i></div>
              </div>
            </div>
          </div>
        </div>

        <div v-if="!conversationListOpen && selectedFileNames.length" class="file-context-strip">
          <span>Только чтение · {{ selectedFileNames.length }} файл(а): {{ selectedFileNames.join(", ") }}</span>
          <button type="button" aria-label="Убрать выбранные файлы" @click="clearSelectedFiles">×</button>
        </div>
        <form v-if="!conversationListOpen" class="drawer-composer" @submit.prevent="submitMessage">
          <label class="composer-attachment" title="Выбрать файлы для локального анализа">
            <span>＋</span>
            <input type="file" multiple accept=".txt,.md,.json,.csv,.log,.rs,.toml,.ts,.tsx,.js,.jsx,.vue,.css,.html,.py,.php,.sql,.yaml,.yml" @change="selectFilesForAnalysis" />
          </label>
          <label class="composer-attachment" title="Выбрать папку для локального анализа">
            <span>▤</span>
            <input type="file" multiple webkitdirectory="" @change="selectFilesForAnalysis" />
          </label>
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

    <div v-if="settingsOpen" class="modal-backdrop" @click.self="settingsOpen = false">
      <section class="permission-modal settings-modal" :aria-label="settingsLabels[interfaceLanguage].settings">
        <header>
          <div class="language-title">
            <span class="language-emblem">⚙</span>
            <div>
              <span class="eyebrow">C-3PO</span>
              <h2>{{ settingsLabels[interfaceLanguage].settings }}</h2>
            </div>
          </div>
          <button type="button" :aria-label="t('close')" @click="settingsOpen = false">×</button>
        </header>

        <div class="settings-content">
          <section class="settings-section">
            <span class="settings-section-title">{{ settingsLabels[interfaceLanguage].appearance }}</span>
            <div class="theme-selector">
              <button type="button" :class="{ active: themePreference === 'system' }" @click="selectTheme('system')">◐ <span>{{ settingsLabels[interfaceLanguage].system }}</span></button>
              <button type="button" :class="{ active: themePreference === 'light' }" @click="selectTheme('light')">☀ <span>{{ settingsLabels[interfaceLanguage].light }}</span></button>
              <button type="button" :class="{ active: themePreference === 'dark' }" @click="selectTheme('dark')">● <span>{{ settingsLabels[interfaceLanguage].dark }}</span></button>
            </div>
          </section>

          <section class="settings-section">
            <span class="settings-section-title">{{ voiceModeLabels[interfaceLanguage].title }}</span>
            <div class="theme-selector voice-mode-selector">
              <button type="button" :class="{ active: voiceMode === 'adaptive' }" @click="selectVoiceMode('adaptive')">◈ <span>{{ voiceModeLabels[interfaceLanguage].adaptive }}</span></button>
              <button type="button" :class="{ active: voiceMode === 'instant' }" @click="selectVoiceMode('instant')">⚡ <span>{{ voiceModeLabels[interfaceLanguage].instant }}</span></button>
              <button type="button" :class="{ active: voiceMode === 'personal' }" @click="selectVoiceMode('personal')">♫ <span>{{ voiceModeLabels[interfaceLanguage].personal }}</span></button>
            </div>
            <small class="settings-hint">
              {{ voiceMode === 'personal' || (voiceMode === 'adaptive' && ttsStatus?.runtimeFound && ttsStatus?.referenceFound)
                ? 'Используется ваш голосовой образец. На слабом компьютере ответ может готовиться дольше.'
                : 'Быстрый голос ОС: ответ начинается сразу и не требует тяжёлой модели.' }}
            </small>
          </section>

          <button class="settings-toggle" type="button" :class="{ active: silentMode }" @click="toggleSilentMode">
            <span class="settings-row-icon">{{ silentMode ? "×" : "♫" }}</span>
            <span><strong>{{ silentModeLabels[interfaceLanguage].title }}</strong><small>{{ silentModeLabels[interfaceLanguage].description }}</small></span>
            <i><b></b></i>
          </button>

          <div class="settings-status-grid">
            <article><span>◉</span><div><strong>{{ settingsLabels[interfaceLanguage].background }}</strong><small>{{ settingsLabels[interfaceLanguage].active }}</small></div><i></i></article>
            <article><span>⌁</span><div><strong>{{ settingsLabels[interfaceLanguage].wake }}</strong><small>C-3PO · C3PO · Си-три-пи-о</small></div><i></i></article>
            <article class="voice-runtime-status">
              <span>♫</span>
              <div><strong>Локальная озвучка</strong><small>{{ ttsStatus?.message || "Проверяю голосовой компонент…" }}</small></div>
              <i :class="{ unavailable: ttsStatus?.state !== 'running' && ttsStatus?.state !== 'stopped' }"></i>
            </article>
          </div>

          <div class="settings-links">
            <button type="button" @click="settingsOpen = false; languageOpen = true"><span>文</span>{{ t("languageSettings") }}<b>→</b></button>
            <button type="button" @click="settingsOpen = false; openPermissions()"><span>⌾</span>{{ t("permissions") }}<b>→</b></button>
            <button type="button" @click="settingsOpen = false; openLlmPanel()"><span>◈</span>{{ llmShortLabel }}<b>→</b></button>
          </div>
        </div>
      </section>
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
            <div><strong>Управление приложениями</strong><small>Нужно для подтверждённой отправки через Telegram</small></div>
            <span class="permission-state prompt">по запросу macOS</span>
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
            <h2>Локальные модели-дроиды</h2>
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

        <section class="droid-model-selector" aria-label="Выбор локальной модели">
          <button
            class="droid-model-card auto"
            type="button"
            :class="{ active: modelPreference === 'auto' }"
            @click="selectDroidModel('auto')"
          >
            <span class="droid-model-avatar">AUTO</span>
            <span><strong>Автовыбор</strong><small>C-3PO выберет модель по задаче</small></span>
            <b>{{ modelPreference === "auto" ? "выбрано" : "выбрать" }}</b>
          </button>
          <button
            v-for="profile in droidModelFleet"
            :key="profile.droid"
            class="droid-model-card"
            type="button"
            :class="{ active: profile.model?.name === modelPreference, unavailable: !profile.model }"
            :disabled="!profile.model"
            @click="profile.model && selectDroidModel(profile.model.name)"
          >
            <span class="droid-model-avatar">{{ profile.droid }}</span>
            <span>
              <strong>{{ profile.droid }}</strong>
              <small>{{ profile.purpose }}</small>
              <em>{{ profile.model ? `${profile.recommendedModel} · ${formatBytes(profile.model.sizeBytes)}` : `${profile.recommendedModel} · не установлена` }}</em>
            </span>
            <b :class="profile.model ? 'ready' : 'missing'">
              {{ profile.model?.name === modelPreference ? "выбрано" : (profile.model ? "готов" : "нет файла") }}
            </b>
          </button>
        </section>

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
              <strong>Модели GGUF</strong>
              <small v-if="llmStatus?.models?.length">
                Установлено: {{ llmStatus.models.filter((model) => model.validGguf).length }} · выбран: {{ selectedDroidName }}
              </small>
              <small v-else>Пока не установлена</small>
            </div>
            <b :class="llmStatus?.selectedModel ? 'ready' : 'missing'">
              {{ llmStatus?.selectedModel ? "готовы" : "нужны" }}
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
          <button
            v-if="(llmStatus?.models?.filter((model) => model.validGguf).length || 0) < 3"
            class="model-primary-action"
            type="button"
            :disabled="llmLoading"
            @click="installRecommendedModels"
          >{{ llmLoading ? "Устанавливаю…" : "Установить 3 модели · ~5,7 ГБ" }}</button>
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
        <p v-if="modelInstallMessage" class="model-install-message">{{ modelInstallMessage }}</p>
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
