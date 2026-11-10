<script setup lang="ts">
import { computed, nextTick, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

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

const input = ref("");
const pendingAction = ref(false);
const isWorking = ref(false);
const messages = ref<Message[]>([
  {
    id: 1,
    role: "assistant",
    text: "Здравствуйте! Я C-3PO, ваш локальный помощник. Пока я умею безопасно показывать информацию об этом компьютере.",
  },
]);
const nextMessageId = ref(2);
const conversation = ref<HTMLElement | null>(null);

const canSend = computed(() => input.value.trim().length > 0 && !isWorking.value);

async function scrollToBottom() {
  await nextTick();
  conversation.value?.scrollTo({
    top: conversation.value.scrollHeight,
    behavior: "smooth",
  });
}

function addMessage(role: Message["role"], text: string) {
  messages.value.push({ id: nextMessageId.value++, role, text });
  void scrollToBottom();
}

function submitMessage() {
  const text = input.value.trim();
  if (!text || isWorking.value) return;

  addMessage("user", text);
  input.value = "";
  pendingAction.value = true;
  void scrollToBottom();
}

function useSuggestion() {
  input.value = "Покажи информацию о системе";
  submitMessage();
}

function cancelAction() {
  pendingAction.value = false;
  addMessage("assistant", "Действие отменено. Никаких изменений в системе не выполнено.");
}

async function loadSystemInfo(): Promise<SystemInfo> {
  if ("__TAURI_INTERNALS__" in window) {
    return invoke<SystemInfo>("get_system_info");
  }

  return {
    operatingSystem: "Режим браузера",
    osVersion: "Демонстрация интерфейса",
    hostName: "localhost",
    architecture: "Apple Silicon",
    totalMemoryMb: 16384,
  };
}

async function approveAction() {
  isWorking.value = true;

  try {
    const info = await loadSystemInfo();
    pendingAction.value = false;
    addMessage(
      "assistant",
      `Готово. Система: ${info.operatingSystem} ${info.osVersion}. Компьютер: ${info.hostName}. Архитектура: ${info.architecture}. Оперативная память: ${Math.round(info.totalMemoryMb / 1024)} ГБ.`,
    );
  } catch (error) {
    pendingAction.value = false;
    addMessage("assistant", `Не удалось получить информацию: ${String(error)}`);
  } finally {
    isWorking.value = false;
  }
}
</script>

<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark" aria-hidden="true">C</div>
        <div>
          <strong>C-3PO</strong>
          <span>Local assistant</span>
        </div>
      </div>

      <button class="new-chat" type="button">
        <span aria-hidden="true">＋</span>
        Новый диалог
      </button>

      <nav class="history" aria-label="История диалогов">
        <p class="section-label">Сегодня</p>
        <button class="history-item active" type="button">
          <span class="chat-icon" aria-hidden="true">◇</span>
          Знакомство с системой
        </button>
      </nav>

      <div class="sidebar-footer">
        <button class="settings-button" type="button">
          <span aria-hidden="true">⚙</span>
          Настройки
        </button>
        <div class="privacy-note">
          <span class="shield" aria-hidden="true">✓</span>
          <div>
            <strong>Данные остаются здесь</strong>
            <span>Локальная обработка</span>
          </div>
        </div>
      </div>
    </aside>

    <main class="workspace">
      <header class="topbar">
        <div>
          <h1>Новый диалог</h1>
          <p>Безопасный помощник для вашего компьютера</p>
        </div>
        <div class="local-status">
          <span class="status-dot"></span>
          Локально
        </div>
      </header>

      <section ref="conversation" class="conversation" aria-live="polite">
        <div class="conversation-inner">
          <div class="welcome">
            <div class="assistant-avatar">C</div>
            <div>
              <span class="eyebrow">ПРОТОКОЛЬНЫЙ ПОМОЩНИК</span>
              <h2>Чем могу помочь?</h2>
              <p>Я предложу действие и попрошу разрешение перед его выполнением.</p>
            </div>
          </div>

          <div
            v-for="message in messages"
            :key="message.id"
            class="message-row"
            :class="message.role"
          >
            <div v-if="message.role === 'assistant'" class="message-avatar">C</div>
            <div class="message-bubble">{{ message.text }}</div>
          </div>

          <button
            v-if="messages.length === 1 && !pendingAction"
            class="suggestion"
            type="button"
            @click="useSuggestion"
          >
            <span class="suggestion-icon" aria-hidden="true">⌁</span>
            <span>
              <strong>Показать информацию о системе</strong>
              <small>Операционная система, процессор и память</small>
            </span>
            <span class="arrow" aria-hidden="true">→</span>
          </button>

          <article v-if="pendingAction" class="action-card">
            <div class="action-heading">
              <div class="action-icon" aria-hidden="true">⌘</div>
              <div>
                <span class="eyebrow">ПРЕДЛАГАЕМОЕ ДЕЙСТВИЕ</span>
                <h3>Получить информацию о системе</h3>
              </div>
              <span class="safe-badge">Безопасно</span>
            </div>
            <p>
              C-3PO прочитает название системы, архитектуру и объём памяти. Файлы и настройки
              не будут изменены.
            </p>
            <div class="action-buttons">
              <button class="secondary-button" type="button" :disabled="isWorking" @click="cancelAction">
                Отмена
              </button>
              <button class="primary-button" type="button" :disabled="isWorking" @click="approveAction">
                {{ isWorking ? "Выполняю…" : "Разрешить" }}
              </button>
            </div>
          </article>
        </div>
      </section>

      <footer class="composer-area">
        <form class="composer" @submit.prevent="submitMessage">
          <textarea
            v-model="input"
            rows="1"
            placeholder="Напишите, что нужно сделать…"
            aria-label="Сообщение для C-3PO"
            @keydown.enter.exact.prevent="submitMessage"
          ></textarea>
          <button class="send-button" type="submit" :disabled="!canSend" aria-label="Отправить">
            ↑
          </button>
        </form>
        <p>C-3PO всегда попросит подтверждение перед изменением данных</p>
      </footer>
    </main>
  </div>
</template>
