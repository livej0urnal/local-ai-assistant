# C-3PO

Локальный компьютерный помощник C-3PO для macOS и Windows.

Интерфейс написан на Vue 3 и TypeScript, а локальная системная логика — на Rust и Tauri 2.
Локальная языковая модель будет подключена через llama.cpp на одном из следующих этапов.

## Установка

```bash
npm install
```

## Запуск интерфейса в браузере

```bash
npm run dev
```

## Запуск настольного приложения

```bash
npm run tauri dev
```

## Проверка проекта

```bash
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```
