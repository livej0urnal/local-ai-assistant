use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ActionResult {
    success: bool,
    message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WeatherReport {
    source: String,
    summary: String,
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
        application
    } else {
        let partial = applications
            .iter()
            .filter(|(name, _)| normalize(name).contains(&query))
            .collect::<Vec<_>>();
        match partial.as_slice() {
            [application] => application,
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
        Ok(selected.0.clone())
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
    let opened_name = launch_named_application(&application)?;
    Ok(ActionResult {
        success: true,
        message: format!("Приложение «{opened_name}» открыто."),
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
    send_telegram(&contact, &message)?;
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
    }

    #[test]
    fn validates_external_message_limits() {
        assert!(validate_message("Любимая", "Я скоро буду").is_ok());
        assert!(validate_message("", "Текст").is_err());
        assert!(validate_message("Контакт", "").is_err());
        assert!(validate_message("Контакт", "строка\nс переносом").is_err());
    }
}
