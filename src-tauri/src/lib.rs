mod executor;
mod llm;
mod productivity;
mod tts;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
    time::{SystemTime, UNIX_EPOCH},
};
use sysinfo::{System, get_current_pid};
use tauri::{
    Manager,
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    webview::{PermissionKind, PermissionResponse},
};
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
struct ProcessMetrics {
    pid: u32,
    resident_memory_bytes: u64,
    virtual_memory_bytes: u64,
    captured_at_unix_ms: u128,
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
    is_app_bundle: bool,
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
static SETTINGS_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct PermissionPreferences {
    microphone_approved: bool,
}

fn permission_preferences_path() -> Result<PathBuf, String> {
    let project_dirs = ProjectDirs::from("com", "igorfilippov", "C-3PO")
        .ok_or_else(|| "Не удалось определить папку настроек приложения".to_string())?;
    let directory = project_dirs.data_local_dir().join("settings");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("permissions.json"))
}

fn read_permission_preferences() -> PermissionPreferences {
    let Ok(path) = permission_preferences_path() else {
        return PermissionPreferences::default();
    };
    let Ok(contents) = fs::read_to_string(path) else {
        return PermissionPreferences::default();
    };
    serde_json::from_str(&contents).unwrap_or_default()
}

#[tauri::command]
fn get_saved_microphone_approval() -> bool {
    read_permission_preferences().microphone_approved
}

#[tauri::command]
fn set_saved_microphone_approval(approved: bool) -> Result<(), String> {
    let _guard = SETTINGS_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "Настройки приложения временно недоступны".to_string())?;
    let preferences = PermissionPreferences {
        microphone_approved: approved,
    };
    let contents = serde_json::to_string_pretty(&preferences).map_err(|error| error.to_string())?;
    fs::write(permission_preferences_path()?, contents).map_err(|error| error.to_string())
}

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
        // На macOS `System::name()` возвращает техническое имя ядра `Darwin`.
        // Пользователю нужен привычный бренд операционной системы.
        operating_system: match std::env::consts::OS {
            "macos" => "macOS".to_string(),
            "windows" => "Windows".to_string(),
            "linux" => System::distribution_id(),
            _ => System::name().unwrap_or_else(|_| std::env::consts::OS.to_string()),
        },
        os_version: System::os_version().unwrap_or_else(|_| "Unknown".to_string()),
        host_name: System::host_name().unwrap_or_else(|_| "Unknown".to_string()),
        architecture: System::cpu_arch(),
        total_memory_mb: system.total_memory() / 1024 / 1024,
    })
}

fn collect_process_metrics() -> Result<ProcessMetrics, String> {
    let pid = get_current_pid().map_err(|error| error.to_string())?;
    let system = System::new_all().map_err(|error| error.to_string())?;
    let process = system
        .process(pid)
        .ok_or_else(|| "Не удалось найти процесс C-3PO".to_string())?;

    Ok(ProcessMetrics {
        pid: pid.as_u32(),
        // RSS — физическая память, действительно находящаяся в ОЗУ сейчас.
        // Это главный показатель для наблюдения за возможной утечкой памяти.
        resident_memory_bytes: process.memory(),
        // Виртуальная память сохраняется для диагностики, но на macOS может
        // быть огромной из-за особенностей адресного пространства WebKit.
        virtual_memory_bytes: process.virtual_memory(),
        captured_at_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
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
        is_app_bundle: std::env::current_exe()
            .map(|path| path.to_string_lossy().contains(".app/Contents/MacOS/"))
            .unwrap_or(false),
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

#[tauri::command]
fn get_process_metrics() -> Result<ProcessMetrics, String> {
    collect_process_metrics()
}

fn reveal_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn show_main_window(app: tauri::AppHandle) {
    reveal_main_window(&app);
}

fn quit_application(app: &tauri::AppHandle) {
    // Закрытие через меню должно завершать не только окно, но и оба фоновых
    // процесса. Обычный крестик по-прежнему лишь скрывает помощника.
    let _ = llm::stop_llm(app.state::<llm::LlmRuntime>());
    let _ = tts::stop_tts(app.state::<tts::TtsRuntime>());
    append_native_log(
        "info",
        "rust.application",
        "C-3PO полностью завершён через меню строки состояния",
    );
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_env_filter(filter).try_init();

    info!("Starting C-3PO");
    append_native_log("info", "rust.application", "Starting C-3PO");

    tauri::Builder::default()
        // Этот плагин намеренно регистрируется первым: повторный запуск не
        // создаёт ещё один WebView и фоновые процессы, а показывает уже
        // существующее главное окно C-3PO.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            append_native_log(
                "info",
                "rust.single-instance",
                "Повторный запуск перенаправлен в существующее окно",
            );
            reveal_main_window(app);
        }))
        .plugin(tauri_plugin_notification::init())
        .manage(llm::LlmRuntime::default())
        .manage(tts::TtsRuntime::default())
        .setup(|app| {
            let status = MenuItemBuilder::with_id("status", "● C-3PO работает в фоне")
                .enabled(false)
                .build(app)?;
            let open = MenuItemBuilder::with_id("open", "Открыть C-3PO").build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Завершить C-3PO").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&status, &open, &quit])
                .build()?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("application icon").clone())
                .tooltip("C-3PO — локальный помощник")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id().as_ref() {
                    "open" => reveal_main_window(app),
                    "quit" => quit_application(app),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        reveal_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .on_permission_request(|_, kind| {
            // Это разрешает запрос внутри WebView, но не обходит защиту ОС:
            // macOS/Windows всё равно показывают пользователю своё системное окно.
            // Все остальные виды доступа используют безопасное поведение по умолчанию.
            let response = match kind {
                PermissionKind::Microphone | PermissionKind::Autoplay => PermissionResponse::Allow,
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
            get_process_metrics,
            show_main_window,
            executor::open_application,
            executor::get_automation_permissions,
            executor::open_automation_settings,
            executor::type_text_in_application,
            executor::search_in_browser,
            executor::get_action_log_path,
            executor::record_action_decision,
            executor::get_current_weather,
            executor::send_telegram_message,
            get_saved_microphone_approval,
            set_saved_microphone_approval,
            llm::get_llm_status,
            llm::install_recommended_models,
            llm::delete_local_model,
            llm::get_llm_runtime_status,
            llm::start_llm,
            llm::stop_llm,
            llm::generate_local_reply,
            productivity::get_mail_overview,
            productivity::get_calendar_overview,
            productivity::create_mail_draft,
            productivity::create_calendar_event,
            tts::get_tts_status,
            tts::start_tts,
            tts::stop_tts,
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

    #[test]
    fn current_process_memory_is_available() {
        let metrics = collect_process_metrics().expect("process metrics should be available");

        assert!(metrics.pid > 0);
        assert!(metrics.resident_memory_bytes > 0);
    }
}
