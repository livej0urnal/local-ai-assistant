use serde::Serialize;
use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static ACTION_LOG_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn action_log_path() -> Result<PathBuf, String> {
    let project_dirs = directories::ProjectDirs::from("com", "igorfilippov", "C-3PO")
        .ok_or_else(|| "Не удалось определить папку журнала действий".to_string())?;
    let directory = project_dirs.data_local_dir().join("logs");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    Ok(directory.join("actions.jsonl"))
}

pub(crate) fn audit_action(action: &str, risk: &str, outcome: &str, metadata: serde_json::Value) {
    let Ok(_guard) = ACTION_LOG_LOCK.get_or_init(|| Mutex::new(())).lock() else {
        return;
    };
    let record = serde_json::json!({
        "timestampUnixMs": SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis()).unwrap_or_default(),
        "action": action,
        "risk": risk,
        "outcome": outcome,
        "metadata": metadata,
    });
    let Ok(path) = action_log_path() else {
        return;
    };
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(file, "{record}");
}

#[tauri::command]
pub(crate) fn get_action_log_path() -> Result<String, String> {
    Ok(action_log_path()?.display().to_string())
}

#[tauri::command]
pub(crate) fn record_action_decision(
    action: String,
    risk: String,
    outcome: String,
) -> Result<(), String> {
    validate_action_decision(&action, &risk, &outcome)?;
    audit_action(&action, &risk, &outcome, serde_json::json!({}));
    Ok(())
}

fn validate_action_decision(action: &str, risk: &str, outcome: &str) -> Result<(), String> {
    const ACTIONS: [&str; 5] = [
        "system-info",
        "telegram-message",
        "application-text",
        "email-draft",
        "calendar-event",
    ];
    const RISKS: [&str; 3] = ["low", "medium", "high"];
    const OUTCOMES: [&str; 2] = ["approved", "cancelled"];
    if !ACTIONS.contains(&action) || !RISKS.contains(&risk) || !OUTCOMES.contains(&outcome) {
        return Err("Некорректная запись журнала действий".to_string());
    }
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionResult {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AutomationPermissions {
    platform: String,
    accessibility: bool,
    screen_recording: bool,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeatherReport {
    source: String,
    summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileSearchReport {
    query: String,
    scope: String,
    paths: Vec<String>,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileContentReport {
    path: String,
    name: String,
    size_bytes: u64,
    content: String,
    truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FileActionResult {
    success: bool,
    message: String,
    undo_kind: Option<String>,
    undo_token: Option<String>,
}

#[derive(Debug, serde::Deserialize, Serialize)]
struct FileMoveUndo {
    from: String,
    to: String,
}

fn user_home() -> Result<PathBuf, String> {
    directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| "Не удалось определить домашнюю папку пользователя".to_string())
}

fn ensure_path_in_home(path: &Path, must_exist: bool) -> Result<PathBuf, String> {
    let home = user_home()?
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let checked = if must_exist {
        path.canonicalize()
            .map_err(|error| format!("Путь не найден: {error}"))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| "У пути отсутствует родительская папка".to_string())?
            .canonicalize()
            .map_err(|error| format!("Папка назначения недоступна: {error}"))?;
        parent.join(
            path.file_name()
                .ok_or_else(|| "Некорректное имя файла".to_string())?,
        )
    };
    if !checked.starts_with(&home) {
        return Err("Разрешены только файлы внутри домашней папки пользователя".to_string());
    }
    Ok(checked)
}

fn standard_directory(reference: &str) -> Option<PathBuf> {
    let dirs = directories::UserDirs::new()?;
    match normalize(reference).as_str() {
        "рабочий стол" | "desktop" => dirs.desktop_dir().map(Path::to_path_buf),
        "документы" | "documents" => dirs.document_dir().map(Path::to_path_buf),
        "загрузки" | "downloads" => dirs.download_dir().map(Path::to_path_buf),
        "музыка" | "music" => dirs.audio_dir().map(Path::to_path_buf),
        "изображения" | "картинки" | "pictures" => {
            dirs.picture_dir().map(Path::to_path_buf)
        }
        "видео" | "videos" => dirs.video_dir().map(Path::to_path_buf),
        "домашняя папка" | "home" => Some(dirs.home_dir().to_path_buf()),
        _ => None,
    }
}

fn validate_file_search_query(value: &str) -> Result<String, String> {
    let query = value.trim();
    if query.is_empty() || query.chars().count() > 160 || query.chars().any(char::is_control) {
        return Err("Укажите часть имени файла или папки длиной до 160 символов".to_string());
    }
    Ok(query.to_string())
}

fn fallback_file_search(home: &Path, query: &str, result_limit: usize) -> Vec<String> {
    let needle = query.to_lowercase();
    let mut stack = vec![home.to_path_buf()];
    let mut results = Vec::new();
    let mut visited = 0usize;
    const VISIT_LIMIT: usize = 80_000;
    const SKIPPED_DIRECTORIES: [&str; 7] = [
        "Library",
        ".git",
        "node_modules",
        "target",
        "dist",
        ".Trash",
        ".cache",
    ];

    while let Some(directory) = stack.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > VISIT_LIMIT || results.len() >= result_limit {
                return results;
            }
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.to_lowercase().contains(&needle) {
                results.push(path.display().to_string());
                if results.len() >= result_limit {
                    return results;
                }
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir()
                && !file_type.is_symlink()
                && !SKIPPED_DIRECTORIES.contains(&name.as_str())
            {
                stack.push(path);
            }
        }
    }
    results
}

fn search_files_native(query: String, limit: Option<usize>) -> Result<FileSearchReport, String> {
    let query = validate_file_search_query(&query)?;
    let limit = limit.unwrap_or(20).clamp(1, 50);
    let home = directories::UserDirs::new()
        .map(|dirs| dirs.home_dir().to_path_buf())
        .ok_or_else(|| "Не удалось определить домашнюю папку пользователя".to_string())?;

    #[cfg(target_os = "macos")]
    let mut paths = {
        // `mdfind -name` ищет по имени в системном индексе Spotlight. Значение
        // передаётся отдельным аргументом и никогда не исполняется как shell-код.
        let output = Command::new("/usr/bin/mdfind")
            .arg("-onlyin")
            .arg(&home)
            .arg("-name")
            .arg(&query)
            .output()
            .map_err(|error| format!("Не удалось запустить локальный поиск: {error}"))?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|path| !path.trim().is_empty())
            .take(limit + 1)
            .map(str::to_string)
            .collect::<Vec<_>>()
    };

    #[cfg(target_os = "macos")]
    if paths.is_empty() {
        paths = fallback_file_search(&home, &query, limit + 1);
    }

    #[cfg(not(target_os = "macos"))]
    let paths: Vec<String> = Vec::new();

    #[cfg(target_os = "windows")]
    if paths.is_empty() {
        return Err(
            "Поиск через Windows Search будет добавлен отдельным нативным адаптером".to_string(),
        );
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    if paths.is_empty() {
        return Err("Системный поиск файлов на этой платформе пока не поддерживается".to_string());
    }

    let truncated = paths.len() > limit;
    let paths = paths.into_iter().take(limit).collect::<Vec<_>>();
    audit_action(
        "file-search",
        "low",
        "completed",
        serde_json::json!({ "results": paths.len(), "truncated": truncated }),
    );
    Ok(FileSearchReport {
        query,
        scope: home.display().to_string(),
        paths,
        truncated,
    })
}

#[tauri::command]
pub(crate) async fn search_files(
    query: String,
    limit: Option<usize>,
) -> Result<FileSearchReport, String> {
    // Прямой обход диска может занять заметное время, поэтому он не должен
    // блокировать окно, анимацию R2-D2 и кнопку остановки голосового ответа.
    tauri::async_runtime::spawn_blocking(move || search_files_native(query, limit))
        .await
        .map_err(|error| format!("Локальный поиск неожиданно остановился: {error}"))?
}

pub(crate) fn resolve_file_reference_native(
    reference: String,
    directory_only: bool,
) -> Result<String, String> {
    let reference = validate_file_search_query(&reference)?;
    if let Some(path) = standard_directory(&reference) {
        let path = ensure_path_in_home(&path, true)?;
        return Ok(path.display().to_string());
    }
    let direct = PathBuf::from(&reference);
    if direct.is_absolute() {
        let path = ensure_path_in_home(&direct, true)?;
        if directory_only && !path.is_dir() {
            return Err("Указанный путь не является папкой".to_string());
        }
        return Ok(path.display().to_string());
    }

    let home = user_home()?;
    let mut matches = fallback_file_search(&home, &reference, 100)
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| !directory_only || path.is_dir())
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        let left_exact = left
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&reference));
        let right_exact = right
            .file_name()
            .is_some_and(|name| name.to_string_lossy().eq_ignore_ascii_case(&reference));
        right_exact
            .cmp(&left_exact)
            .then_with(|| left.components().count().cmp(&right.components().count()))
            .then_with(|| left.cmp(right))
    });
    let path = matches
        .into_iter()
        .next()
        .ok_or_else(|| format!("Не найден путь по имени «{reference}»"))?;
    Ok(path.display().to_string())
}

#[tauri::command]
pub(crate) async fn resolve_file_reference(
    reference: String,
    directory_only: bool,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        resolve_file_reference_native(reference, directory_only)
    })
    .await
    .map_err(|error| format!("Поиск пути неожиданно остановился: {error}"))?
}

fn read_text_file_native(path: String) -> Result<FileContentReport, String> {
    const MAX_FILE_BYTES: u64 = 2_000_000;
    const MAX_CONTENT_CHARS: usize = 24_000;
    const TEXT_EXTENSIONS: [&str; 28] = [
        "txt", "md", "json", "csv", "log", "rs", "toml", "ts", "tsx", "js", "jsx", "vue", "css",
        "html", "htm", "py", "php", "sql", "yaml", "yml", "xml", "sh", "zsh", "java", "kt",
        "swift", "c", "cpp",
    ];
    let path = ensure_path_in_home(Path::new(path.trim()), true)?;
    if !path.is_file() {
        return Err("Указанный путь не является файлом".to_string());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if !TEXT_EXTENSIONS.contains(&extension.as_str()) {
        return Err(format!(
            "Формат .{extension} пока нельзя безопасно прочитать как текст"
        ));
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err("Файл больше 2 МБ. Для анализа выберите меньший файл".to_string());
    }
    let bytes = fs::read(&path).map_err(|error| format!("Не удалось прочитать файл: {error}"))?;
    let text = String::from_utf8(bytes)
        .map_err(|_| "Файл не является текстом UTF-8".to_string())?
        .replace('\0', "");
    let content = text.chars().take(MAX_CONTENT_CHARS).collect::<String>();
    let truncated = text.chars().count() > MAX_CONTENT_CHARS;
    audit_action(
        "file-read",
        "low",
        "completed",
        serde_json::json!({ "file": path.file_name(), "bytes": metadata.len(), "truncated": truncated }),
    );
    Ok(FileContentReport {
        name: path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        path: path.display().to_string(),
        size_bytes: metadata.len(),
        content,
        truncated,
    })
}

#[tauri::command]
pub(crate) async fn read_text_file(path: String) -> Result<FileContentReport, String> {
    tauri::async_runtime::spawn_blocking(move || read_text_file_native(path))
        .await
        .map_err(|error| format!("Чтение файла неожиданно остановилось: {error}"))?
}

fn move_file_native(
    source: String,
    destination_directory: String,
) -> Result<FileActionResult, String> {
    let source = ensure_path_in_home(Path::new(source.trim()), true)?;
    let destination_directory = ensure_path_in_home(Path::new(destination_directory.trim()), true)?;
    if !destination_directory.is_dir() {
        return Err("Путь назначения не является папкой".to_string());
    }
    if source.is_dir() && destination_directory.starts_with(&source) {
        return Err("Нельзя переместить папку внутрь самой себя".to_string());
    }
    let file_name = source
        .file_name()
        .ok_or_else(|| "Нельзя переместить корневую папку".to_string())?;
    let destination = destination_directory.join(file_name);
    ensure_path_in_home(&destination, false)?;
    if destination.exists() {
        return Err("В папке назначения уже существует объект с таким именем".to_string());
    }
    fs::rename(&source, &destination)
        .map_err(|error| format!("Не удалось переместить файл или папку: {error}"))?;
    let undo = FileMoveUndo {
        from: destination.display().to_string(),
        to: source.display().to_string(),
    };
    audit_action(
        "file-move",
        "medium",
        "completed",
        serde_json::json!({ "from": source, "to": destination }),
    );
    Ok(FileActionResult {
        success: true,
        message: format!(
            "«{}» перемещён в «{}».",
            file_name.to_string_lossy(),
            destination_directory.display()
        ),
        undo_kind: Some("file-move".to_string()),
        undo_token: Some(serde_json::to_string(&undo).map_err(|error| error.to_string())?),
    })
}

#[tauri::command]
pub(crate) async fn move_file(
    source: String,
    destination_directory: String,
) -> Result<FileActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || move_file_native(source, destination_directory))
        .await
        .map_err(|error| format!("Перемещение неожиданно остановилось: {error}"))?
}

fn validate_file_name(value: &str) -> Result<String, String> {
    let name = value.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().count() > 255
        || name
            .chars()
            .any(|character| character.is_control() || character == '/' || character == '\\')
    {
        return Err("Некорректное новое имя файла или папки".to_string());
    }
    Ok(name.to_string())
}

fn rename_file_native(source: String, new_name: String) -> Result<FileActionResult, String> {
    let source = ensure_path_in_home(Path::new(source.trim()), true)?;
    let new_name = validate_file_name(&new_name)?;
    let destination = source
        .parent()
        .ok_or_else(|| "Нельзя переименовать корневую папку".to_string())?
        .join(&new_name);
    ensure_path_in_home(&destination, false)?;
    if destination.exists() {
        return Err("Объект с новым именем уже существует".to_string());
    }
    let previous_name = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    fs::rename(&source, &destination)
        .map_err(|error| format!("Не удалось переименовать файл или папку: {error}"))?;
    let undo = FileMoveUndo {
        from: destination.display().to_string(),
        to: source.display().to_string(),
    };
    audit_action(
        "file-rename",
        "medium",
        "completed",
        serde_json::json!({ "from": source, "to": destination }),
    );
    Ok(FileActionResult {
        success: true,
        message: format!("«{previous_name}» переименован в «{new_name}»."),
        undo_kind: Some("file-move".to_string()),
        undo_token: Some(serde_json::to_string(&undo).map_err(|error| error.to_string())?),
    })
}

#[tauri::command]
pub(crate) async fn rename_file(
    source: String,
    new_name: String,
) -> Result<FileActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || rename_file_native(source, new_name))
        .await
        .map_err(|error| format!("Переименование неожиданно остановилось: {error}"))?
}

fn undo_file_action_native(token: String) -> Result<FileActionResult, String> {
    let undo: FileMoveUndo =
        serde_json::from_str(&token).map_err(|_| "Повреждена запись отмены".to_string())?;
    let from = ensure_path_in_home(Path::new(&undo.from), true)?;
    let to = ensure_path_in_home(Path::new(&undo.to), false)?;
    if to.exists() {
        return Err("Исходный путь уже занят — автоматическая отмена остановлена".to_string());
    }
    fs::rename(&from, &to).map_err(|error| format!("Не удалось отменить перемещение: {error}"))?;
    audit_action(
        "undo-file-move",
        "low",
        "completed",
        serde_json::json!({ "from": from, "to": to }),
    );
    Ok(FileActionResult {
        success: true,
        message: "Последнее перемещение отменено.".to_string(),
        undo_kind: None,
        undo_token: None,
    })
}

#[tauri::command]
pub(crate) async fn undo_file_action(token: String) -> Result<FileActionResult, String> {
    tauri::async_runtime::spawn_blocking(move || undo_file_action_native(token))
        .await
        .map_err(|error| format!("Отмена неожиданно остановилась: {error}"))?
}

fn normalize(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace(['-', '_'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn canonical_application_name(value: &str) -> String {
    match normalize(value).as_str() {
        "telegram" | "телеграм" | "телеграмм" => "Telegram".to_string(),
        "whatsapp" | "ватсап" | "вацап" | "вотсап" => "WhatsApp".to_string(),
        "chrome" | "google chrome" | "гугл хром" | "хром" => {
            "Google Chrome".to_string()
        }
        "safari" | "сафари" => "Safari".to_string(),
        "mail" | "почта" | "email" => "Mail".to_string(),
        "finder" | "проводник" | "файлы" | "files" => {
            if cfg!(target_os = "windows") {
                "File Explorer"
            } else {
                "Finder"
            }
            .to_string()
        }
        "notes" | "заметки" | "блокнот" => if cfg!(target_os = "windows") {
            "Notepad"
        } else {
            "Notes"
        }
        .to_string(),
        "calendar" | "календарь" => "Calendar".to_string(),
        "calculator" | "калькулятор" => "Calculator".to_string(),
        "settings" | "настройки" | "параметры" => {
            if cfg!(target_os = "windows") {
                "Settings"
            } else {
                "System Settings"
            }
            .to_string()
        }
        "weather" | "погода" => "Weather".to_string(),
        // Для любого другого приложения сохраняем произнесённое имя. Оно не
        // исполняется как команда, а сравнивается с системным каталогом.
        _ => value.trim().to_string(),
    }
}

fn known_website(value: &str) -> Option<(&'static str, &'static str)> {
    match normalize(value).as_str() {
        "youtube" | "ютуб" => Some(("YouTube", "https://www.youtube.com")),
        "gmail" | "гугл почта" => Some(("Gmail", "https://mail.google.com")),
        "google" | "гугл" => Some(("Google", "https://www.google.com")),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn collect_macos_bundles(directory: &Path, depth: usize, paths: &mut BTreeSet<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
        {
            if path.is_dir() {
                paths.insert(path);
            }
        } else if path.is_dir() {
            collect_macos_bundles(&path, depth - 1, paths);
        }
    }
}

#[cfg(target_os = "macos")]
fn installed_applications() -> Vec<(String, PathBuf)> {
    let mut paths = BTreeSet::new();
    // Spotlight возвращает зарегистрированные bundle-пути, включая приложения
    // пользователя. Запрос является константой и не содержит пользовательский текст.
    if let Ok(output) = Command::new("/usr/bin/mdfind")
        .arg("kMDItemContentType == 'com.apple.application-bundle'")
        .output()
    {
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let path = PathBuf::from(line.trim());
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("app"))
                && path.is_dir()
            {
                paths.insert(path);
            }
        }
    }
    // На компьютерах с выключенным Spotlight используем предсказуемый обход
    // стандартных каталогов и никогда не заходим внутрь найденного .app-bundle.
    for root in [
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        dirs_home().join("Applications"),
    ] {
        collect_macos_bundles(&root, 5, &mut paths);
    }
    paths
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem()?.to_string_lossy().trim().to_string();
            (!name.is_empty()).then_some((name, path))
        })
        .collect()
}

#[cfg(target_os = "macos")]
fn dirs_home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

#[cfg(target_os = "macos")]
fn launch_named_application(requested: &str) -> Result<String, String> {
    if let Some((name, url)) = known_website(requested) {
        Command::new("/usr/bin/open")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Не удалось открыть {name}: {error}"))?;
        return Ok(format!("{name} в браузере"));
    }
    let requested = canonical_application_name(requested);
    if normalize(&requested) == "browser" || normalize(&requested) == "браузер" {
        Command::new("/usr/bin/open")
            .arg("https://www.google.com")
            .spawn()
            .map_err(|error| format!("Не удалось открыть браузер: {error}"))?;
        return Ok("браузер по умолчанию".to_string());
    }
    let query = normalize(&requested);
    let applications = installed_applications();
    let exact = applications
        .iter()
        .find(|(name, _)| normalize(name) == query);
    let selected = if let Some(application) = exact {
        application.clone()
    } else {
        // Некоторые системные приложения (например Safari в новых macOS)
        // находятся в защищённом Cryptex и не видны обычному обходу каталогов.
        // LaunchServices умеет разрешить их точное зарегистрированное имя.
        let launch_services = Command::new("/usr/bin/open")
            .args(["-a", requested.as_str()])
            .output()
            .map_err(|error| format!("Не удалось обратиться к LaunchServices: {error}"))?;
        if launch_services.status.success() {
            return Ok(requested);
        }
        let partial = applications
            .iter()
            .filter(|(name, _)| normalize(name).contains(&query))
            .collect::<Vec<_>>();
        match partial.as_slice() {
            [application] => (*application).clone(),
            [] => {
                return Err(format!(
                    "Приложение «{requested}» не найдено среди установленных."
                ));
            }
            _ => {
                let names = partial
                    .iter()
                    .take(5)
                    .map(|(name, _)| name.as_str())
                    .collect::<Vec<_>>();
                return Err(format!(
                    "Найдено несколько приложений: {}. Назовите точнее.",
                    names.join(", ")
                ));
            }
        }
    };
    let output = Command::new("/usr/bin/open")
        .arg(&selected.1)
        .output()
        .map_err(|error| format!("Не удалось запустить {}: {error}", selected.0))?;
    if output.status.success() {
        Ok(selected.0)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(target_os = "windows")]
fn collect_windows_shortcuts(directory: &Path, depth: usize, result: &mut Vec<(String, PathBuf)>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_windows_shortcuts(&path, depth - 1, result);
        } else if path.extension().is_some_and(|extension| {
            extension.eq_ignore_ascii_case("lnk") || extension.eq_ignore_ascii_case("appref-ms")
        }) && let Some(name) = path
            .file_stem()
            .map(|name| name.to_string_lossy().trim().to_string())
        {
            if !name.is_empty() {
                result.push((name, path));
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn launch_named_application(requested: &str) -> Result<String, String> {
    if let Some((name, url)) = known_website(requested) {
        Command::new("explorer.exe")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Не удалось открыть {name}: {error}"))?;
        return Ok(format!("{name} в браузере"));
    }
    let requested = canonical_application_name(requested);
    let query = normalize(&requested);
    let mut applications = Vec::new();
    for variable in ["PROGRAMDATA", "APPDATA"] {
        if let Ok(root) = std::env::var(variable) {
            collect_windows_shortcuts(
                &PathBuf::from(root).join("Microsoft/Windows/Start Menu/Programs"),
                6,
                &mut applications,
            );
        }
    }
    if let Some((name, path)) = applications
        .iter()
        .find(|(name, _)| normalize(name) == query)
    {
        Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Не удалось открыть {name}: {error}"))?;
        return Ok(name.clone());
    }
    let known = match query.as_str() {
        "file explorer" => Some(("explorer.exe", "")),
        "notepad" => Some(("notepad.exe", "")),
        "calculator" => Some(("calc.exe", "")),
        "settings" => Some(("explorer.exe", "ms-settings:")),
        "mail" => Some(("explorer.exe", "mailto:")),
        "calendar" => Some(("explorer.exe", "outlookcal:")),
        "weather" => Some(("explorer.exe", "msnweather:")),
        "browser" | "браузер" => Some(("explorer.exe", "https://www.google.com")),
        "telegram" => Some(("explorer.exe", "tg://")),
        "whatsapp" => Some(("explorer.exe", "whatsapp://")),
        _ => None,
    };
    if let Some((program, argument)) = known {
        let mut command = Command::new(program);
        if !argument.is_empty() {
            command.arg(argument);
        }
        command
            .spawn()
            .map_err(|error| format!("Не удалось открыть {requested}: {error}"))?;
        return Ok(requested);
    }
    Err(format!("Приложение «{requested}» не найдено в меню Пуск."))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn launch_named_application(_requested: &str) -> Result<String, String> {
    Err("Запуск приложений пока поддерживается только на macOS и Windows".to_string())
}

#[tauri::command]
pub(crate) fn open_application(application: String) -> Result<ActionResult, String> {
    if application.trim().is_empty()
        || application.chars().count() > 120
        || application.chars().any(char::is_control)
    {
        return Err("Укажите корректное название приложения".to_string());
    }
    let opened_name = launch_named_application(&application).map_err(|error| {
        audit_action(
            "open-application",
            "low",
            "failed",
            serde_json::json!({ "application": application.clone() }),
        );
        error
    })?;
    audit_action(
        "open-application",
        "low",
        "completed",
        serde_json::json!({ "application": opened_name.clone() }),
    );
    Ok(ActionResult {
        success: true,
        message: format!("Приложение «{opened_name}» открыто."),
    })
}

#[cfg(target_os = "macos")]
#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPreflightScreenCaptureAccess() -> bool;
}

#[tauri::command]
pub(crate) fn get_automation_permissions() -> AutomationPermissions {
    #[cfg(target_os = "macos")]
    {
        // Обе функции только читают состояние TCC и не показывают системных окон.
        let accessibility = unsafe { AXIsProcessTrusted() };
        let screen_recording = unsafe { CGPreflightScreenCaptureAccess() };
        return AutomationPermissions {
            platform: "macos".to_string(),
            accessibility,
            screen_recording,
            message: if accessibility {
                "Управление интерфейсом разрешено.".to_string()
            } else {
                "Для клавиатуры и кнопок разрешите C-3PO в разделе «Универсальный доступ»."
                    .to_string()
            },
        };
    }
    #[cfg(target_os = "windows")]
    {
        AutomationPermissions {
            platform: "windows".to_string(),
            accessibility: true,
            screen_recording: true,
            message: "Доступность действий проверяется при выполнении через Windows UI Automation."
                .to_string(),
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        AutomationPermissions {
            platform: std::env::consts::OS.to_string(),
            accessibility: false,
            screen_recording: false,
            message: "Автоматизация на этой платформе пока не поддерживается.".to_string(),
        }
    }
}

#[tauri::command]
pub(crate) fn open_automation_settings(kind: String) -> Result<ActionResult, String> {
    #[cfg(target_os = "macos")]
    {
        let url = match kind.as_str() {
            "accessibility" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
            }
            "screen-recording" => {
                "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            }
            _ => return Err("Неизвестный раздел системных разрешений".to_string()),
        };
        Command::new("/usr/bin/open")
            .arg(url)
            .spawn()
            .map_err(|error| format!("Не удалось открыть системные настройки: {error}"))?;
        return Ok(ActionResult {
            success: true,
            message: "Открыт системный раздел разрешений.".to_string(),
        });
    }
    #[cfg(target_os = "windows")]
    {
        let _ = kind;
        Command::new("explorer.exe")
            .arg("ms-settings:privacy")
            .spawn()
            .map_err(|error| format!("Не удалось открыть параметры Windows: {error}"))?;
        Ok(ActionResult {
            success: true,
            message: "Открыты параметры конфиденциальности Windows.".to_string(),
        })
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = kind;
        Err("Системные настройки разрешений на этой платформе не поддерживаются".to_string())
    }
}

fn validate_automation_text(application: &str, text: &str) -> Result<(String, String), String> {
    let application = application.trim();
    let text = text.trim();
    if application.is_empty() || application.chars().count() > 120 {
        return Err("Некорректное название приложения".to_string());
    }
    if text.is_empty() || text.chars().count() > 4_000 || text.chars().any(char::is_control) {
        return Err(
            "Текст должен содержать от 1 до 4000 символов без управляющих символов".to_string(),
        );
    }
    Ok((application.to_string(), text.to_string()))
}

#[tauri::command]
pub(crate) fn type_text_in_application(
    application: String,
    text: String,
    submit: bool,
) -> Result<ActionResult, String> {
    let (application, text) = validate_automation_text(&application, &text)?;
    #[cfg(target_os = "macos")]
    {
        if !unsafe { AXIsProcessTrusted() } {
            return Err("macOS не разрешила управление интерфейсом. Откройте C-3PO в Системные настройки → Конфиденциальность и безопасность → Универсальный доступ.".to_string());
        }
        launch_named_application(&application)?;
        const SCRIPT: &str = r#"
on run argv
  set messageText to item 1 of argv
  set shouldSubmit to item 2 of argv
  set previousClipboard to the clipboard
  try
    delay 0.6
    tell application "System Events"
      set the clipboard to messageText
      keystroke "v" using command down
      if shouldSubmit is "true" then key code 36
    end tell
    set the clipboard to previousClipboard
  on error errorMessage number errorNumber
    set the clipboard to previousClipboard
    error errorMessage number errorNumber
  end try
end run
"#;
        let output = Command::new("/usr/bin/osascript")
            .args(["-e", SCRIPT, &text, if submit { "true" } else { "false" }])
            .output()
            .map_err(|error| format!("Не удалось выполнить ввод: {error}"))?;
        if !output.status.success() {
            audit_action(
                "application-text",
                if submit { "high" } else { "medium" },
                "failed",
                serde_json::json!({
                    "application": application.clone(),
                    "characters": text.chars().count(),
                    "submitted": submit,
                }),
            );
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }
        audit_action(
            "application-text",
            if submit { "high" } else { "medium" },
            "completed",
            serde_json::json!({
                "application": application.clone(),
                "characters": text.chars().count(),
                "submitted": submit,
            }),
        );
        return Ok(ActionResult {
            success: true,
            message: if submit {
                format!("Текст введён и подтверждён в «{application}».")
            } else {
                format!("Текст введён в «{application}».")
            },
        });
    }
    #[cfg(target_os = "windows")]
    {
        let _ = (application, text, submit);
        Err("Безопасный ввод через Windows UI Automation будет добавлен отдельным нативным модулем.".to_string())
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = (application, text, submit);
        Err("Управление интерфейсом на этой платформе пока не поддерживается".to_string())
    }
}

fn percent_encode_query(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "+".to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect()
}

#[tauri::command]
pub(crate) fn search_in_browser(browser: String, query: String) -> Result<ActionResult, String> {
    let browser = browser.trim();
    let query = query.trim();
    if browser.is_empty()
        || browser.chars().count() > 120
        || query.is_empty()
        || query.chars().count() > 500
    {
        return Err("Укажите браузер и поисковый запрос".to_string());
    }
    let url = format!(
        "https://www.google.com/search?q={}",
        percent_encode_query(query)
    );
    #[cfg(target_os = "macos")]
    let result = Command::new("/usr/bin/open")
        .args([
            "-a",
            canonical_application_name(browser).as_str(),
            url.as_str(),
        ])
        .output();
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer.exe").arg(&url).output();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result: std::io::Result<std::process::Output> =
        Err(std::io::Error::other("unsupported platform"));
    let output = result.map_err(|error| format!("Не удалось открыть поиск: {error}"))?;
    if !output.status.success() {
        audit_action(
            "browser-search",
            "low",
            "failed",
            serde_json::json!({
                "browser": canonical_application_name(browser),
                "queryCharacters": query.chars().count(),
            }),
        );
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    audit_action(
        "browser-search",
        "low",
        "completed",
        serde_json::json!({
            "browser": canonical_application_name(browser),
            "queryCharacters": query.chars().count(),
        }),
    );
    Ok(ActionResult {
        success: true,
        message: format!("Поиск «{query}» открыт в браузере."),
    })
}

fn first_text(value: &serde_json::Value, path: &[&str]) -> Option<String> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
        if let Some(items) = current.as_array() {
            current = items.first()?;
        }
    }
    current
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn weather_report_from_payload(payload: &serde_json::Value) -> Result<WeatherReport, String> {
    // Некоторые версии wttr.in оборачивают прежний ответ в поле data.
    let data = payload.get("data").unwrap_or(payload);
    let current = data
        .get("current_condition")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .ok_or_else(|| "Сервис погоды не вернул текущие условия".to_string())?;
    let area = data
        .get("nearest_area")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first());

    let city = area
        .and_then(|value| first_text(value, &["areaName", "value"]))
        .unwrap_or_else(|| "текущее местоположение".to_string());
    let country = area.and_then(|value| first_text(value, &["country", "value"]));
    let place = country.map_or(city.clone(), |country| format!("{city}, {country}"));
    let temperature = first_text(current, &["temp_C"])
        .ok_or_else(|| "Сервис погоды не вернул температуру".to_string())?;
    let feels_like = first_text(current, &["FeelsLikeC"]);
    let humidity = first_text(current, &["humidity"]);
    let wind = first_text(current, &["windspeedKmph"]);
    let description = first_text(current, &["lang_ru", "value"])
        .or_else(|| first_text(current, &["weatherDesc", "value"]))
        .unwrap_or_else(|| "условия не указаны".to_string());
    let observed_at = first_text(current, &["localObsDateTime"]);

    let mut details = vec![format!("{place}: {description}, {temperature} °C")];
    if let Some(value) = feels_like {
        details.push(format!("ощущается как {value} °C"));
    }
    if let Some(value) = humidity {
        details.push(format!("влажность {value}%"));
    }
    if let Some(value) = wind {
        details.push(format!("ветер {value} км/ч"));
    }
    if let Some(value) = observed_at {
        details.push(format!("наблюдение {value}"));
    }

    Ok(WeatherReport {
        source: "wttr.in (местоположение определено по сети)".to_string(),
        summary: details.join(", "),
    })
}

fn read_current_weather() -> Result<WeatherReport, String> {
    // Фоновый HTTPS-запрос не запускает системное приложение и не требует
    // доступа к экрану. Сервис видит обычный сетевой адрес, но не текст команды.
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(12)))
        .build()
        .new_agent();
    let mut response = agent
        .get("https://wttr.in/?format=j1&lang=ru")
        .header("User-Agent", "C-3PO-local-assistant/0.1")
        .call()
        .map_err(|error| format!("Фоновый сервис погоды недоступен: {error}"))?;
    let payload: serde_json::Value = response
        .body_mut()
        .read_json()
        .map_err(|error| format!("Не удалось прочитать ответ сервиса погоды: {error}"))?;
    weather_report_from_payload(&payload)
}

#[tauri::command]
pub(crate) async fn get_current_weather() -> Result<WeatherReport, String> {
    tauri::async_runtime::spawn_blocking(read_current_weather)
        .await
        .map_err(|error| format!("Не удалось дождаться данных погоды: {error}"))?
}

fn validate_message(contact: &str, message: &str) -> Result<(String, String), String> {
    let contact = contact.trim();
    let message = message.trim();
    if contact.is_empty() || contact.chars().count() > 80 {
        return Err("Имя контакта должно содержать от 1 до 80 символов".to_string());
    }
    if message.is_empty() || message.chars().count() > 4_000 {
        return Err("Сообщение должно содержать от 1 до 4000 символов".to_string());
    }
    if contact.chars().any(char::is_control) || message.chars().any(char::is_control) {
        return Err("Контакт или сообщение содержат недопустимые управляющие символы".to_string());
    }
    Ok((contact.to_string(), message.to_string()))
}

#[cfg(target_os = "macos")]
fn send_telegram(contact: &str, message: &str) -> Result<(), String> {
    // Скрипт является константой, а пользовательские строки передаются как
    // отдельные argv. Поэтому имя контакта и текст не могут стать AppleScript-кодом.
    const SCRIPT: &str = r#"
on run argv
  set contactName to item 1 of argv
  set messageText to item 2 of argv
  set previousClipboard to the clipboard
  try
    tell application "Telegram" to activate
    delay 0.7
    tell application "System Events"
      keystroke "k" using command down
      delay 0.4
      set the clipboard to contactName
      keystroke "v" using command down
      delay 1.0
      key code 36
      delay 0.8
      set the clipboard to messageText
      keystroke "v" using command down
      delay 0.2
      key code 36
    end tell
    set the clipboard to previousClipboard
  on error errorMessage number errorNumber
    set the clipboard to previousClipboard
    error errorMessage number errorNumber
  end try
end run
"#;
    launch_named_application("Telegram")?;
    let output = Command::new("/usr/bin/osascript")
        .args(["-e", SCRIPT, contact, message])
        .output()
        .map_err(|error| format!("Не удалось управлять Telegram: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if error.contains("not allowed assistive access") || error.contains("-25211") {
            Err("macOS не разрешила управление Telegram. Откройте Системные настройки → Конфиденциальность и безопасность → Универсальный доступ и разрешите C-3PO.".to_string())
        } else {
            Err(format!("Telegram не подтвердил отправку: {error}"))
        }
    }
}

#[cfg(target_os = "windows")]
fn send_telegram(_contact: &str, _message: &str) -> Result<(), String> {
    Err("На Windows безопасная отправка в Telegram пока не подключена. C-3PO может открыть Telegram, но не будет имитировать нажатия без отдельного модуля UI Automation.".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn send_telegram(_contact: &str, _message: &str) -> Result<(), String> {
    Err("Отправка Telegram пока поддерживается только на macOS".to_string())
}

#[tauri::command]
pub(crate) fn send_telegram_message(
    contact: String,
    message: String,
) -> Result<ActionResult, String> {
    let (contact, message) = validate_message(&contact, &message)?;
    send_telegram(&contact, &message).map_err(|error| {
        audit_action(
            "telegram-message",
            "high",
            "failed",
            serde_json::json!({
                "contactCharacters": contact.chars().count(),
                "messageCharacters": message.chars().count(),
            }),
        );
        error
    })?;
    audit_action(
        "telegram-message",
        "high",
        "completed",
        serde_json::json!({
            "contactCharacters": contact.chars().count(),
            "messageCharacters": message.chars().count(),
        }),
    );
    Ok(ActionResult {
        success: true,
        message: format!("Сообщение контакту «{contact}» отправлено через Telegram."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_background_weather_response() {
        let payload = serde_json::json!({
            "current_condition": [{
                "temp_C": "18",
                "FeelsLikeC": "17",
                "humidity": "61",
                "windspeedKmph": "9",
                "weatherDesc": [{ "value": "Partly cloudy" }],
                "localObsDateTime": "2026-11-10 07:00 PM"
            }],
            "nearest_area": [{
                "areaName": [{ "value": "Podgorica" }],
                "country": [{ "value": "Montenegro" }]
            }]
        });

        let report = weather_report_from_payload(&payload).expect("weather report");
        assert!(report.summary.contains("Podgorica, Montenegro"));
        assert!(report.summary.contains("18 °C"));
        assert!(report.summary.contains("влажность 61%"));
    }

    #[test]
    fn resolves_application_aliases_without_replacing_chrome_with_safari() {
        assert_eq!(canonical_application_name("Телеграмм"), "Telegram");
        assert_eq!(canonical_application_name("WhatsApp"), "WhatsApp");
        assert_eq!(canonical_application_name("хром"), "Google Chrome");
        assert_eq!(canonical_application_name("Safari"), "Safari");
        assert_eq!(
            canonical_application_name("неизвестная программа"),
            "неизвестная программа"
        );
        assert_eq!(
            known_website("ютуб"),
            Some(("YouTube", "https://www.youtube.com"))
        );
    }

    #[test]
    fn validates_external_message_limits() {
        assert!(validate_message("Любимая", "Я скоро буду").is_ok());
        assert!(validate_message("", "Текст").is_err());
        assert!(validate_message("Контакт", "").is_err());
        assert!(validate_message("Контакт", "строка\nс переносом").is_err());
    }

    #[test]
    fn encodes_search_queries_without_shell_interpretation() {
        assert_eq!(percent_encode_query("C-3PO rust"), "C-3PO+rust");
        assert_eq!(
            percent_encode_query("привет"),
            "%D0%BF%D1%80%D0%B8%D0%B2%D0%B5%D1%82"
        );
    }

    #[test]
    fn validates_confirmed_application_text() {
        assert!(validate_automation_text("Notes", "Купить молоко").is_ok());
        assert!(validate_automation_text("", "Текст").is_err());
        assert!(validate_automation_text("Notes", "строка\nс переносом").is_err());
    }

    #[test]
    fn rejects_forged_action_journal_values() {
        assert!(validate_action_decision("system-info", "low", "approved").is_ok());
        assert!(validate_action_decision("shell", "low", "approved").is_err());
        assert!(validate_action_decision("system-info", "unknown", "approved").is_err());
        assert!(validate_action_decision("system-info", "low", "rewritten").is_err());
    }

    #[test]
    fn validates_local_file_search_queries() {
        assert_eq!(
            validate_file_search_query(" README.md ").unwrap(),
            "README.md"
        );
        assert!(validate_file_search_query("").is_err());
        assert!(validate_file_search_query("bad\nquery").is_err());
        assert!(validate_file_search_query(&"x".repeat(161)).is_err());
    }

    #[test]
    fn fallback_search_finds_files_without_an_index() {
        let root = std::env::temp_dir().join(format!("c3po-file-search-{}", std::process::id()));
        let nested = root.join("Documents");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("Quarterly Report.txt"), "test").unwrap();
        let results = fallback_file_search(&root, "quarterly", 5);
        assert_eq!(results.len(), 1);
        assert!(results[0].ends_with("Quarterly Report.txt"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_moves_and_undoes_a_text_file() {
        let home = user_home().unwrap();
        let root = tempfile::Builder::new()
            .prefix("c3po-file-actions-")
            .tempdir_in(home)
            .unwrap();
        let destination = root.path().join("Archive");
        fs::create_dir(&destination).unwrap();
        let source = root.path().join("notes.txt");
        fs::write(&source, "Локальный тест").unwrap();

        let report = read_text_file_native(source.display().to_string()).unwrap();
        assert_eq!(report.content, "Локальный тест");

        let moved = move_file_native(
            source.display().to_string(),
            destination.display().to_string(),
        )
        .unwrap();
        assert!(!source.exists());
        assert!(destination.join("notes.txt").exists());

        undo_file_action_native(moved.undo_token.unwrap()).unwrap();
        assert!(source.exists());
        assert!(!destination.join("notes.txt").exists());

        let renamed =
            rename_file_native(source.display().to_string(), "ideas.md".to_string()).unwrap();
        assert!(!source.exists());
        assert!(root.path().join("ideas.md").exists());
        undo_file_action_native(renamed.undo_token.unwrap()).unwrap();
        assert!(source.exists());
    }
}
