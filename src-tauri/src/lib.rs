use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use sysinfo::System;
use tauri::webview::{PermissionKind, PermissionResponse};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemInfo {
    operating_system: String,
    os_version: String,
    host_name: String,
    architecture: String,
    total_memory_mb: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeCapability {
    id: String,
    available: bool,
    permission_required: bool,
    state: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvironmentReport {
    system: SystemInfo,
    platform: String,
    family: String,
    capabilities: Vec<NativeCapability>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClientLogEntry {
    level: String,
    source: String,
    message: String,
    details: Option<String>,
    timestamp: String,
}

static LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

// Храним журнал в системной папке данных приложения, а не рядом с программой:
// установленное приложение может не иметь права записывать файлы в свою папку.
fn application_log_path() -> Result<PathBuf, String> {
    let project_dirs = ProjectDirs::from("com", "igorfilippov", "C-3PO")
        .ok_or_else(|| "Не удалось определить папку данных приложения".to_string())?;
    let directory = project_dirs.data_local_dir().join("logs");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("c3po.log"))
}

fn clean_log_value(value: &str, max_chars: usize) -> String {
    // Одна запись должна занимать одну строку. Так журнал остаётся читаемым,
    // а случайный длинный текст ошибки не сможет бесконечно увеличивать файл.
    value
        .chars()
        .take(max_chars)
        .map(|character| {
            if character == '\n' || character == '\r' {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn append_log_line(
    timestamp: &str,
    level: &str,
    source: &str,
    message: &str,
    details: Option<&str>,
) -> Result<(), String> {
    let _guard = LOG_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Журнал ошибок временно недоступен".to_string())?;
    let path = application_log_path()?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| error.to_string())?;

    let details = details.filter(|value| !value.is_empty()).unwrap_or("—");
    writeln!(
        file,
        "{} | {:<5} | {} | {} | {}",
        clean_log_value(timestamp, 64),
        clean_log_value(level, 12).to_uppercase(),
        clean_log_value(source, 80),
        clean_log_value(message, 1000),
        clean_log_value(details, 4000),
    )
    .map_err(|error| error.to_string())
}

fn append_native_log(level: &str, source: &str, message: &str) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "unknown-time".to_string());
    if let Err(error) = append_log_line(&timestamp, level, source, message, None) {
        warn!(%error, "Could not write application log");
    }
}

#[tauri::command]
fn log_client_event(entry: ClientLogEntry) -> Result<(), String> {
    append_log_line(
        &entry.timestamp,
        &entry.level,
        &entry.source,
        &entry.message,
        entry.details.as_deref(),
    )
}

#[tauri::command]
fn get_recent_logs(limit: Option<usize>) -> Result<Vec<String>, String> {
    let path = application_log_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let limit = limit.unwrap_or(40).clamp(1, 200);
    let mut lines: Vec<String> = contents
        .lines()
        .rev()
        .take(limit)
        .map(str::to_string)
        .collect();
    lines.reverse();
    Ok(lines)
}

#[tauri::command]
fn get_log_path() -> Result<String, String> {
    Ok(application_log_path()?.display().to_string())
}

fn collect_system_info() -> Result<SystemInfo, String> {
    let mut system = System::new_all().map_err(|error| error.to_string())?;
    system.refresh_all();

    Ok(SystemInfo {
        operating_system: System::name().unwrap_or_else(|_| "Unknown".to_string()),
        os_version: System::os_version().unwrap_or_else(|_| "Unknown".to_string()),
        host_name: System::host_name().unwrap_or_else(|_| "Unknown".to_string()),
        architecture: System::cpu_arch(),
        total_memory_mb: system.total_memory() / 1024 / 1024,
    })
}

#[tauri::command]
fn get_system_info() -> Result<SystemInfo, String> {
    info!("Collecting system information");

    collect_system_info()
}

#[tauri::command]
fn get_environment_report() -> Result<EnvironmentReport, String> {
    info!("Collecting environment and native capability information");

    let desktop = matches!(std::env::consts::OS, "macos" | "windows" | "linux");

    Ok(EnvironmentReport {
        system: collect_system_info()?,
        platform: std::env::consts::OS.to_string(),
        family: std::env::consts::FAMILY.to_string(),
        capabilities: vec![
            NativeCapability {
                id: "system-info".to_string(),
                available: true,
                permission_required: false,
                state: "available".to_string(),
            },
            NativeCapability {
                id: "microphone".to_string(),
                available: desktop,
                permission_required: true,
                state: "requires-runtime-check".to_string(),
            },
            NativeCapability {
                id: "files".to_string(),
                available: desktop,
                permission_required: true,
                state: "on-demand".to_string(),
            },
            NativeCapability {
                id: "screen-capture".to_string(),
                available: desktop,
                permission_required: true,
                state: "not-requested".to_string(),
            },
            NativeCapability {
                id: "accessibility".to_string(),
                available: matches!(std::env::consts::OS, "macos" | "windows"),
                permission_required: true,
                state: "not-requested".to_string(),
            },
        ],
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    info!("Starting C-3PO");
    append_native_log("info", "rust.application", "Starting C-3PO");

    tauri::Builder::default()
        .on_permission_request(|_, kind| {
            // Это разрешает запрос внутри WebView, но не обходит защиту ОС:
            // macOS/Windows всё равно показывают пользователю своё системное окно.
            // Все остальные виды доступа используют безопасное поведение по умолчанию.
            let response = match kind {
                PermissionKind::Microphone => PermissionResponse::Allow,
                _ => PermissionResponse::Default,
            };
            append_native_log(
                "info",
                "tauri.permission",
                &format!("WebView requested {kind}; response: {response}"),
            );
            response
        })
        .invoke_handler(tauri::generate_handler![
            get_system_info,
            get_environment_report,
            log_client_event,
            get_recent_logs,
            get_log_path
        ])
        .run(tauri::generate_context!())
        .expect("failed to run C-3PO");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_information_is_available() {
        let info = get_system_info().expect("system information should be available");

        assert!(!info.operating_system.is_empty());
        assert!(!info.architecture.is_empty());
        assert!(info.total_memory_mb > 0);
    }

    #[test]
    fn environment_report_matches_build_target() {
        let report = get_environment_report().expect("environment report should be available");

        assert_eq!(report.platform, std::env::consts::OS);
        assert_eq!(report.family, std::env::consts::FAMILY);
        assert!(
            report
                .capabilities
                .iter()
                .any(|item| item.id == "microphone")
        );
    }
}
