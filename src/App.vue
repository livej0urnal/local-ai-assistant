<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import droidPortrait from "./assets/protocol-droid.png";

type Message = {
  id: number;
  role: "assistant" | "user";
  text: string;
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

const messages = ref<Message[]>([
  {
    id: 1,
    role: "assistant",
    text: "Здравствуйте. Голосовой режим готов. Скажите: «Покажи информацию о системе».",
  },
]);
const nextMessageId = ref(2);
const input = ref("");
const liveTranscript = ref("");
const lastCommand = ref("Скажите команду…");
const pendingAction = ref(false);
const isWorking = ref(false);
const chatOpen = ref(false);
const permissionsOpen = ref(false);
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
const processMetrics = ref<ProcessMetrics | null>(null);
const memoryBaselineBytes = ref(0);
const memoryPeakBytes = ref(0);

let recognition: RecognitionLike | null = null;
let microphoneStream: MediaStream | null = null;
let audioContext: AudioContext | null = null;
let meterFrame = 0;
let restartTimer = 0;
let shouldListen = false;
let memoryTimer = 0;

const canSend = computed(() => input.value.trim().length > 0 && !isWorking.value);
const systemLabel = computed(() => {
  if (!environment.value) return "Определяю систему…";
  return `${environment.value.system.operatingSystem} ${environment.value.system.osVersion}`;
});
const voiceStatus = computed(() => {
  if (voiceState.value === "idle" && microphoneActive.value && !recognitionAvailable.value) {
    return "Микрофон включён";
  }
  const labels: Record<VoiceState, string> = {
    idle: "Готов к голосовой команде",
    listening: "Слушаю вас",
    thinking: "Обрабатываю команду",
    speaking: "Отвечаю голосом",
    paused: "Микрофон на паузе",
    error: "Голосовой режим недоступен",
  };
  return labels[voiceState.value];
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

function formatBytes(bytes: number) {
  const absolute = Math.abs(bytes);
  if (absolute < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} КБ`;
  return `${(bytes / 1024 / 1024).toFixed(1)} МБ`;
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
  await refreshLogs();
}

async function scrollChat() {
  await nextTick();
  conversation.value?.scrollTo({ top: conversation.value.scrollHeight, behavior: "smooth" });
}

function addMessage(role: Message["role"], text: string) {
  messages.value.push({ id: nextMessageId.value++, role, text });
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

function configureRecognition() {
  const Recognition = getRecognitionConstructor();
  recognitionAvailable.value = Boolean(Recognition);
  synthesisAvailable.value = "speechSynthesis" in window;

  if (!Recognition) return;

  recognition = new Recognition();
  recognition.lang = "ru-RU";
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
    if (finalText.trim()) processVoiceCommand(finalText.trim());
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
    microphonePermission.value = "granted";
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
    microphonePermission.value = details.name === "NotAllowedError" ? "denied" : "unsupported";
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

  try {
    const status = await navigator.permissions.query({ name: "microphone" as PermissionName });
    microphonePermission.value = status.state;
    status.onchange = () => {
      microphonePermission.value = status.state;
      if (status.state === "granted") void requestMicrophone();
    };
  } catch (error) {
    microphonePermission.value = "prompt";
    const details = errorDetails(error);
    await writeLog("warn", "microphone.permission-check", `${details.name}: ${details.message}`);
  }

  if (microphonePermission.value === "granted") await requestMicrophone();
  else onboardingOpen.value = true;
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
  utterance.voice = voices.find((voice) => voice.lang.toLowerCase().startsWith("ru")) ?? null;
  utterance.lang = "ru-RU";
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

function proposeSystemInfo() {
  pendingAction.value = true;
  voiceState.value = "idle";
  const response = "Я могу прочитать сведения о системе. Скажите «выполнить» или нажмите кнопку подтверждения.";
  addMessage("assistant", response);
  speak(response);
}

function cancelAction() {
  pendingAction.value = false;
  const response = "Действие отменено. Никаких изменений не выполнено.";
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
    const response = `Не удалось получить информацию: ${String(error)}`;
    addMessage("assistant", response);
    speak(response);
  } finally {
    isWorking.value = false;
  }
}

function processVoiceCommand(text: string) {
  const normalized = text.toLowerCase();
  lastCommand.value = text;
  liveTranscript.value = "";
  addMessage("user", text);

  if (pendingAction.value && /(выполн|разреш|подтверж|да)/.test(normalized)) {
    void approveAction();
    return;
  }
  if (pendingAction.value && /(отмен|нет|стоп)/.test(normalized)) {
    cancelAction();
    return;
  }
  if (/(систем|компьютер|операцион|памят|процессор)/.test(normalized)) {
    proposeSystemInfo();
    return;
  }
  if (/(открой|покажи).*(чат|диалог)/.test(normalized)) {
    chatOpen.value = true;
    const response = "Открываю текстовый диалог.";
    addMessage("assistant", response);
    speak(response);
    return;
  }
  if (/(закрой|скрой).*(чат|диалог)/.test(normalized)) {
    chatOpen.value = false;
    speak("Диалог закрыт. Продолжаю слушать голосовые команды.");
    return;
  }

  const response = "Эту команду я пока не умею выполнять. Сейчас доступна информация о системе.";
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

onMounted(async () => {
  configureRecognition();
  await loadEnvironment();
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
});
</script>

<template>
  <div class="voice-app" :class="`state-${voiceState}`">
    <header class="app-header">
      <div class="compact-brand">
        <div class="brand-symbol">C</div>
        <div>
          <strong>C-3PO</strong>
          <span>Локальный помощник</span>
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
        <button class="ghost-button" type="button" @click="openPermissions">
          <span class="button-icon">⌾</span>
          Доступы
          <i :class="microphonePermission"></i>
        </button>
        <button class="chat-button" type="button" @click="chatOpen = true">
          <span>Открыть диалог</span>
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
          <span class="eyebrow">ГОЛОСОВОЙ РЕЖИМ</span>
          <h1>{{ voiceStatus }}</h1>
          <p class="transcript">{{ liveTranscript || lastCommand }}</p>

          <button
            class="primary-microphone"
            :class="{ active: microphoneActive }"
            type="button"
            @click="toggleMicrophone"
          >
            <span class="microphone-icon"></span>
            <span>{{ canPauseMicrophone ? "Поставить на паузу" : "Включить микрофон" }}</span>
          </button>

          <p v-if="microphonePermission === 'granted' && !recognitionAvailable" class="voice-warning">
            Микрофон доступен, но системное распознавание речи не поддерживается этим WebView.
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
            <span class="eyebrow">БЫСТРЫЙ СТАРТ</span>
            <h2>Что можно сказать</h2>
          </div>
          <span class="privacy-badge">Без передачи файлов</span>
        </div>

        <div class="command-grid">
          <button type="button" @click="processVoiceCommand('Покажи информацию о системе')">
            <span class="command-icon">⌘</span>
            <span>
              <strong>«Покажи информацию о системе»</strong>
              <small>ОС, архитектура и оперативная память</small>
            </span>
            <b>→</b>
          </button>
          <button type="button" @click="chatOpen = true">
            <span class="command-icon">◇</span>
            <span>
              <strong>«Открой диалог»</strong>
              <small>Показать текстовую историю справа</small>
            </span>
            <b>→</b>
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
      <aside class="chat-drawer" aria-label="Текстовый диалог">
        <header>
          <div>
            <span class="eyebrow">ИСТОРИЯ</span>
            <h2>Диалог с C-3PO</h2>
          </div>
          <button type="button" aria-label="Закрыть диалог" @click="chatOpen = false">×</button>
        </header>

        <div ref="conversation" class="drawer-messages">
          <div v-for="message in messages" :key="message.id" class="drawer-message" :class="message.role">
            <span v-if="message.role === 'assistant'" class="small-avatar">C</span>
            <p>{{ message.text }}</p>
          </div>
        </div>

        <form class="drawer-composer" @submit.prevent="submitMessage">
          <textarea
            v-model="input"
            rows="2"
            placeholder="Введите команду…"
            aria-label="Текстовая команда"
            @keydown.enter.exact.prevent="submitMessage"
          ></textarea>
          <button type="submit" :disabled="!canSend">↑</button>
        </form>
      </aside>
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
            <span class="permission-state" :class="permissionStatus('microphone')">{{ permissionStatus('microphone') }}</span>
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
              <small>Содержит технические сведения, но не записывает звук или текст команд</small>
            </div>
            <button type="button" :disabled="logsLoading" @click="refreshLogs">
              {{ logsLoading ? "Читаю…" : "Обновить" }}
            </button>
          </header>
          <pre v-if="recentLogs.length">{{ recentLogs.join("\n") }}</pre>
          <p v-else>Записей пока нет.</p>
          <small v-if="logPath" class="log-path">Файл: {{ logPath }}</small>
        </section>

        <p class="permission-note">
          C-3PO не запрашивает права заранее, если функция ими не пользуется. Это уменьшает доступ
          приложения к системе.
        </p>
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
