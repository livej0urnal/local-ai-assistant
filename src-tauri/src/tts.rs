use directories::ProjectDirs;
use serde::Serialize;
use std::{
    env,
    fs::{self, OpenOptions},
    net::{SocketAddr, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Mutex,
    time::Duration,
};
use tauri::State;

const TTS_HOST: &str = "127.0.0.1";
const TTS_PORT: u16 = 11438;
const SERVER_SOURCE: &str = include_str!("../../scripts/local_tts_server.py");

#[derive(Default)]
pub(crate) struct TtsRuntime {
    child: Mutex<Option<Child>>,
}

impl Drop for TtsRuntime {
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
pub(crate) struct TtsStatus {
    state: String,
    runtime_found: bool,
    reference_found: bool,
    process_id: Option<u32>,
    address: String,
    data_directory: String,
    reference_path: Option<String>,
    message: String,
}

fn tts_directory() -> Result<PathBuf, String> {
    let project_dirs = ProjectDirs::from("com", "igorfilippov", "C-3PO")
        .ok_or_else(|| "Не удалось определить папку локального голоса".to_string())?;
    let directory = project_dirs.data_local_dir().join("tts");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory)
}

fn python_executable(directory: &Path) -> Option<PathBuf> {
    if let Ok(value) = env::var("C3PO_TTS_PYTHON") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    let candidates = if cfg!(windows) {
        vec![directory.join("chatterbox-venv/Scripts/python.exe")]
    } else {
        vec![
            directory.join("chatterbox-venv/bin/python"),
            directory.join("chatterbox-venv/bin/python3.11"),
        ]
    };
    candidates.into_iter().find(|path| path.is_file())
}

fn reference_file(directory: &Path) -> Option<PathBuf> {
    if let Ok(value) = env::var("C3PO_TTS_REFERENCE") {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Some(path);
        }
    }
    ["wav", "mp3", "m4a", "flac", "ogg"]
        .iter()
        .map(|extension| directory.join(format!("reference.{extension}")))
        .find(|path| path.is_file())
}

fn server_address() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], TTS_PORT))
}

fn server_is_running() -> bool {
    TcpStream::connect_timeout(&server_address(), Duration::from_millis(150)).is_ok()
}

fn status(runtime: &TtsRuntime) -> Result<TtsStatus, String> {
    let directory = tts_directory()?;
    let python = python_executable(&directory);
    let reference = reference_file(&directory);
    let mut child_guard = runtime
        .child
        .lock()
        .map_err(|_| "Состояние локального голоса временно недоступно".to_string())?;
    let mut process_id = None;
    if let Some(child) = child_guard.as_mut() {
        if child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            child_guard.take();
        } else {
            process_id = Some(child.id());
        }
    }
    let running = server_is_running();
    let (state, message) = if running {
        ("running", "Локальный голос готов озвучивать полные ответы.")
    } else if python.is_none() {
        (
            "runtime-missing",
            "Установите локальный голосовой компонент из scripts/setup-local-voice.py.",
        )
    } else if reference.is_none() {
        (
            "reference-missing",
            "Выберите собственный или лицензированный образец голоса.",
        )
    } else if process_id.is_some() {
        ("starting", "Локальный голосовой процесс запускается…")
    } else {
        ("stopped", "Локальный голос установлен и готов к запуску.")
    };
    Ok(TtsStatus {
        state: state.to_string(),
        runtime_found: python.is_some(),
        reference_found: reference.is_some(),
        process_id,
        address: format!("http://{TTS_HOST}:{TTS_PORT}"),
        data_directory: directory.display().to_string(),
        reference_path: reference.map(|path| path.display().to_string()),
        message: message.to_string(),
    })
}

#[tauri::command]
pub(crate) fn get_tts_status(runtime: State<'_, TtsRuntime>) -> Result<TtsStatus, String> {
    status(&runtime)
}

#[tauri::command]
pub(crate) fn start_tts(runtime: State<'_, TtsRuntime>) -> Result<TtsStatus, String> {
    if server_is_running() {
        return status(&runtime);
    }
    let directory = tts_directory()?;
    let python = python_executable(&directory)
        .ok_or_else(|| "Локальный голосовой компонент не установлен".to_string())?;
    let reference = reference_file(&directory)
        .ok_or_else(|| "Не найден приватный образец голоса".to_string())?;
    let server_script = directory.join("local_tts_server.py");
    fs::write(&server_script, SERVER_SOURCE).map_err(|error| error.to_string())?;
    fs::create_dir_all(directory.join("numba-cache")).map_err(|error| error.to_string())?;

    let log_directory = directory
        .parent()
        .ok_or_else(|| "Не удалось определить папку журналов".to_string())?
        .join("logs");
    fs::create_dir_all(&log_directory).map_err(|error| error.to_string())?;
    let stdout = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_directory.join("tts-server.log"))
        .map_err(|error| error.to_string())?;
    let stderr = stdout.try_clone().map_err(|error| error.to_string())?;

    let mut child_guard = runtime
        .child
        .lock()
        .map_err(|_| "Состояние локального голоса временно недоступно".to_string())?;
    if let Some(child) = child_guard.as_mut()
        && child
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
    {
        drop(child_guard);
        return status(&runtime);
    }
    // Передаём пути отдельными аргументами окружения, без shell-строки:
    // пробелы в имени пользователя или папки не меняют смысл команды.
    let child = Command::new(python)
        .arg(server_script)
        .env("C3PO_TTS_REFERENCE", reference)
        .env("C3PO_TTS_PORT", TTS_PORT.to_string())
        .env("NUMBA_CACHE_DIR", directory.join("numba-cache"))
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr))
        .spawn()
        .map_err(|error| format!("Не удалось запустить локальный голос: {error}"))?;
    *child_guard = Some(child);
    drop(child_guard);
    status(&runtime)
}

#[tauri::command]
pub(crate) fn stop_tts(runtime: State<'_, TtsRuntime>) -> Result<TtsStatus, String> {
    let mut child_guard = runtime
        .child
        .lock()
        .map_err(|_| "Состояние локального голоса временно недоступно".to_string())?;
    if let Some(mut child) = child_guard.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    drop(child_guard);
    status(&runtime)
}
