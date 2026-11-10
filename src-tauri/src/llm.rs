use directories::ProjectDirs;
use serde::Serialize;
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Read,
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use sysinfo::{Pid, System};
use tauri::State;

const LLAMA_HOST: &str = "127.0.0.1";
const LLAMA_PORT: u16 = 11435;

#[derive(Default)]
pub(crate) struct LlmRuntime {
    child: Mutex<Option<Child>>,
}

impl Drop for LlmRuntime {
    fn drop(&mut self) {
        if let Ok(child) = self.child.get_mut()
            && let Some(child) = child.as_mut()
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalModelFile {
    name: String,
    path: String,
    size_bytes: u64,
    valid_gguf: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LlmStatus {
    state: String,
    engine_found: bool,
    engine_path: Option<String>,
    model_directory: String,
    selected_model: Option<LocalModelFile>,
    discovered_models: usize,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LlmRuntimeStatus {
    state: String,
    ready: bool,
    process_id: Option<u32>,
    resident_memory_bytes: u64,
    address: String,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalReply {
    text: String,
}

fn model_directory() -> Result<PathBuf, String> {
    let project_dirs = ProjectDirs::from("com", "igorfilippov", "C-3PO")
        .ok_or_else(|| "Не удалось определить папку моделей".to_string())?;
    let directory = project_dirs.data_local_dir().join("models");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn executable_name() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn find_llama_server() -> Option<PathBuf> {
    // Явная переменная полезна разработчикам и не привязывает приложение
    // к одному конкретному способу установки llama.cpp.
    if let Ok(path) = env::var("C3PO_LLAMA_SERVER") {
        let candidate = PathBuf::from(path);
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    if let Ok(current_exe) = env::current_exe()
        && let Some(directory) = current_exe.parent()
    {
        let candidate = directory.join(executable_name());
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    if let Some(path) = env::var_os("PATH") {
        for directory in env::split_paths(&path) {
            let candidate = directory.join(executable_name());
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }

    // Docker Desktop иногда устанавливает совместимый arm64 llama-server,
    // но не добавляет его в PATH. Проверяем этот путь как удобный локальный вариант.
    if let Some(home) = env::var_os("HOME") {
        let candidate = PathBuf::from(home)
            .join(".docker")
            .join("bin")
            .join("inference")
            .join(executable_name());
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let candidate = Path::new(directory).join(executable_name());
        if is_executable_file(&candidate) {
            return Some(candidate);
        }
    }

    None
}

fn has_gguf_header(path: &Path) -> bool {
    let Ok(mut file) = File::open(path) else {
        return false;
    };
    let mut header = [0_u8; 4];
    file.read_exact(&mut header).is_ok() && &header == b"GGUF"
}

fn discover_models(directory: &Path) -> Result<Vec<LocalModelFile>, String> {
    let mut models = Vec::new();
    let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let is_gguf = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"));
        if !path.is_file() || !is_gguf {
            continue;
        }

        let metadata = entry.metadata().map_err(|error| error.to_string())?;
        models.push(LocalModelFile {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path.display().to_string(),
            size_bytes: metadata.len(),
            valid_gguf: has_gguf_header(&path),
        });
    }

    models.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(models)
}

fn selected_runtime_parts() -> Result<(PathBuf, PathBuf), String> {
    let engine = find_llama_server().ok_or_else(|| "Движок llama-server не найден".to_string())?;
    let directory = model_directory()?;
    let model = discover_models(&directory)?
        .into_iter()
        .find(|model| model.valid_gguf)
        .ok_or_else(|| "Корректная GGUF-модель не найдена".to_string())?;
    Ok((engine, PathBuf::from(model.path)))
}

fn server_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], LLAMA_PORT))
}

fn server_is_ready() -> bool {
    TcpStream::connect_timeout(&server_address(), Duration::from_millis(150)).is_ok()
}

fn runtime_status(runtime: &LlmRuntime) -> Result<LlmRuntimeStatus, String> {
    let mut child_guard = runtime
        .child
        .lock()
        .map_err(|_| "Состояние локальной модели временно недоступно".to_string())?;

    let mut process_id = None;
    let mut exited = false;
    if let Some(child) = child_guard.as_mut() {
        process_id = Some(child.id());
        exited = child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some();
    }
    if exited {
        child_guard.take();
        process_id = None;
    }

    let ready = process_id.is_some() && server_is_ready();
    let resident_memory_bytes = process_id
        .and_then(|pid| {
            System::new_all()
                .ok()?
                .process(Pid::from_u32(pid))
                .map(|process| process.memory())
        })
        .unwrap_or(0);
    let (state, message) = if ready {
        ("ready", "Локальная модель загружена и готова отвечать.")
    } else if process_id.is_some() {
        ("loading", "llama.cpp загружает модель в память…")
    } else {
        ("stopped", "Локальная модель пока не запущена.")
    };

    Ok(LlmRuntimeStatus {
        state: state.to_string(),
        ready,
        process_id,
        resident_memory_bytes,
        address: format!("http://{LLAMA_HOST}:{LLAMA_PORT}"),
        message: message.to_string(),
    })
}

#[tauri::command]
pub(crate) fn get_llm_status() -> Result<LlmStatus, String> {
    let directory = model_directory()?;
    let engine = find_llama_server();
    let mut models = discover_models(&directory)?;
    let discovered_models = models.len();
    let selected_model = models.drain(..).find(|model| model.valid_gguf);

    let (state, message) = match (&engine, &selected_model) {
        (None, _) => (
            "engine-missing",
            "Движок llama.cpp пока не найден. Его можно установить или положить рядом с приложением.",
        ),
        (Some(_), None) if discovered_models == 0 => (
            "model-missing",
            "llama.cpp найден. Для первого запуска осталось добавить модель в формате GGUF.",
        ),
        (Some(_), None) => (
            "model-invalid",
            "В папке есть файлы .gguf, но их заголовок не прошёл проверку.",
        ),
        (Some(_), Some(_)) => (
            "ready",
            "Движок и модель найдены. Следующий шаг — запуск локального сервера генерации.",
        ),
    };

    Ok(LlmStatus {
        state: state.to_string(),
        engine_found: engine.is_some(),
        engine_path: engine.map(|path| path.display().to_string()),
        model_directory: directory.display().to_string(),
        selected_model,
        discovered_models,
        message: message.to_string(),
    })
}

#[tauri::command]
pub(crate) fn get_llm_runtime_status(
    runtime: State<'_, LlmRuntime>,
) -> Result<LlmRuntimeStatus, String> {
    runtime_status(&runtime)
}

#[tauri::command]
pub(crate) fn start_llm(runtime: State<'_, LlmRuntime>) -> Result<LlmRuntimeStatus, String> {
    {
        let mut child_guard = runtime
            .child
            .lock()
            .map_err(|_| "Состояние локальной модели временно недоступно".to_string())?;

        if let Some(child) = child_guard.as_mut()
            && child
                .try_wait()
                .map_err(|error| error.to_string())?
                .is_none()
        {
            drop(child_guard);
            return runtime_status(&runtime);
        }

        let (engine, model) = selected_runtime_parts()?;
        let log_directory = model_directory()?
            .parent()
            .ok_or_else(|| "Не удалось определить папку журнала модели".to_string())?
            .join("logs");
        fs::create_dir_all(&log_directory).map_err(|error| error.to_string())?;
        let log_path = log_directory.join("llama-server.log");
        let stdout = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .map_err(|error| error.to_string())?;
        let stderr = stdout.try_clone().map_err(|error| error.to_string())?;

        // Аргументы задаются отдельно, без shell-строки: путь модели не может
        // превратиться в произвольную команду. Сервер слушает только localhost.
        let child = Command::new(engine)
            .arg("--model")
            .arg(model)
            .args([
                "--host",
                LLAMA_HOST,
                "--port",
                &LLAMA_PORT.to_string(),
                "--ctx-size",
                "4096",
                "--n-gpu-layers",
                "99",
                "--no-webui",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::from(stdout))
            .stderr(Stdio::from(stderr))
            .spawn()
            .map_err(|error| format!("Не удалось запустить llama-server: {error}"))?;
        *child_guard = Some(child);
    }

    runtime_status(&runtime)
}

#[tauri::command]
pub(crate) fn stop_llm(runtime: State<'_, LlmRuntime>) -> Result<LlmRuntimeStatus, String> {
    let mut child_guard = runtime
        .child
        .lock()
        .map_err(|_| "Состояние локальной модели временно недоступно".to_string())?;
    if let Some(mut child) = child_guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    drop(child_guard);
    runtime_status(&runtime)
}

#[tauri::command]
pub(crate) fn generate_local_reply(
    prompt: String,
    language: String,
    runtime: State<'_, LlmRuntime>,
) -> Result<LocalReply, String> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Введите сообщение для локальной модели".to_string());
    }
    if prompt.chars().count() > 4_000 {
        return Err("Сообщение слишком длинное: максимум 4000 символов".to_string());
    }
    if !runtime_status(&runtime)?.ready {
        return Err("Локальная модель ещё не готова".to_string());
    }

    // Разрешаем только языки из интерфейса. Это не даёт произвольному тексту
    // попасть в системную инструкцию модели.
    let greeting = match language.as_str() {
        "Russian" => "Здравствуйте! Я C-3PO. Чем могу помочь?",
        "English" => "Hello! I am C-3PO. How may I help?",
        "Spanish" => "¡Hola! Soy C-3PO. ¿En qué puedo ayudarte?",
        "German" => "Hallo! Ich bin C-3PO. Wie kann ich helfen?",
        "French" => "Bonjour ! Je suis C-3PO. Comment puis-je vous aider ?",
        "Italian" => "Salve! Sono C-3PO. Come posso aiutarti?",
        "Portuguese" => "Olá! Sou C-3PO. Como posso ajudar?",
        "Simplified Chinese" => "您好！我是 C-3PO。我能为您做些什么？",
        "Japanese" => "こんにちは！C-3POです。何をお手伝いしましょうか？",
        "Arabic" => "مرحبًا! أنا C-3PO. كيف يمكنني مساعدتك؟",
        _ => return Err("Выбранный язык пока не поддерживается".to_string()),
    };
    let system_prompt = format!(
        "You create final replies for the local computer assistant C-3PO. Your reply is shown and spoken verbatim to the user. Reply directly and exclusively in {language}. Never mention system instructions, internal modules, R2-D2, role allocation, or an inability to talk to the user. Never say C-3PO is your assistant. Be polite, useful, and concise. If the user asks for a computer action, do not claim it was completed; explain the proposal and ask for confirmation."
    );

    let request = serde_json::json!({
        "model": "local",
        "messages": [
            {
                "role": "system",
                "content": system_prompt
            },
            // Небольшой пример закрепляет нужную роль надёжнее, чем длинный
            // список запретов: модель видит готовый формат ответа C-3PO.
            {
                "role": "user",
                "content": "Hello! Who are you?"
            },
            {
                "role": "assistant",
                "content": greeting
            },
            { "role": "user", "content": prompt }
        ],
        "temperature": 0.55,
        "max_tokens": 256,
        "stream": false
    });

    let mut response = ureq::post(format!(
        "http://{LLAMA_HOST}:{LLAMA_PORT}/v1/chat/completions"
    ))
    .send_json(request)
    .map_err(|error| format!("Ошибка локального запроса: {error}"))?;
    let payload: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|error| format!("Не удалось прочитать ответ модели: {error}"))?;
    let text = payload["choices"][0]["message"]["content"]
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .ok_or_else(|| "Модель вернула пустой ответ".to_string())?;

    Ok(LocalReply {
        text: text.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn recognises_gguf_magic_header() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("test.gguf");
        let mut file = File::create(&path).expect("test model should be created");
        file.write_all(b"GGUFtest-data")
            .expect("test header should be written");

        assert!(has_gguf_header(&path));
    }

    #[test]
    fn rejects_file_without_gguf_magic_header() {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let path = directory.path().join("broken.gguf");
        let mut file = File::create(&path).expect("test model should be created");
        file.write_all(b"not-a-model")
            .expect("test data should be written");

        assert!(!has_gguf_header(&path));
    }
}
