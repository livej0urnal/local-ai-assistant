use serde::Serialize;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApplicationId {
    Telegram,
    WhatsApp,
    Browser,
    Mail,
    Files,
    Notes,
    Calendar,
    Calculator,
    Settings,
    Weather,
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

fn application_id(value: &str) -> Option<ApplicationId> {
    match normalize(value).as_str() {
        "telegram" | "телеграм" | "телеграмм" => Some(ApplicationId::Telegram),
        "whatsapp" | "ватсап" | "вацап" | "вотсап" => {
            Some(ApplicationId::WhatsApp)
        }
        "browser" | "браузер" | "safari" | "chrome" | "google chrome" => {
            Some(ApplicationId::Browser)
        }
        "mail" | "почта" | "email" => Some(ApplicationId::Mail),
        "finder" | "проводник" | "файлы" | "files" => Some(ApplicationId::Files),
        "notes" | "заметки" | "блокнот" => Some(ApplicationId::Notes),
        "calendar" | "календарь" => Some(ApplicationId::Calendar),
        "calculator" | "калькулятор" => Some(ApplicationId::Calculator),
        "settings" | "настройки" | "параметры" => Some(ApplicationId::Settings),
        "weather" | "погода" => Some(ApplicationId::Weather),
        _ => None,
    }
}

#[cfg(target_os = "macos")]
fn launch(application: ApplicationId) -> Result<(), String> {
    let name = match application {
        ApplicationId::Telegram => "Telegram",
        ApplicationId::WhatsApp => "WhatsApp",
        ApplicationId::Browser => "Safari",
        ApplicationId::Mail => "Mail",
        ApplicationId::Files => "Finder",
        ApplicationId::Notes => "Notes",
        ApplicationId::Calendar => "Calendar",
        ApplicationId::Calculator => "Calculator",
        ApplicationId::Settings => "System Settings",
        ApplicationId::Weather => "Weather",
    };
    let output = Command::new("/usr/bin/open")
        .args(["-a", name])
        .output()
        .map_err(|error| format!("Не удалось запустить {name}: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

#[cfg(target_os = "windows")]
fn launch(application: ApplicationId) -> Result<(), String> {
    let (program, argument) = match application {
        ApplicationId::Telegram => ("explorer.exe", "tg://"),
        ApplicationId::WhatsApp => ("explorer.exe", "whatsapp://"),
        ApplicationId::Browser => ("explorer.exe", "https://www.google.com"),
        ApplicationId::Mail => ("explorer.exe", "mailto:"),
        ApplicationId::Files => ("explorer.exe", "."),
        ApplicationId::Notes => ("notepad.exe", ""),
        ApplicationId::Calendar => ("explorer.exe", "outlookcal:"),
        ApplicationId::Calculator => ("calc.exe", ""),
        ApplicationId::Settings => ("explorer.exe", "ms-settings:"),
        ApplicationId::Weather => ("explorer.exe", "msnweather:"),
    };
    let mut command = Command::new(program);
    if !argument.is_empty() {
        command.arg(argument);
    }
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("Не удалось запустить приложение: {error}"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn launch(_application: ApplicationId) -> Result<(), String> {
    Err("Запуск приложений пока поддерживается только на macOS и Windows".to_string())
}

#[tauri::command]
pub(crate) fn open_application(application: String) -> Result<ActionResult, String> {
    let application_id = application_id(&application).ok_or_else(|| {
        "Приложение отсутствует в безопасном каталоге C-3PO. Доступны Telegram, WhatsApp, браузер, почта, файлы, заметки, календарь, калькулятор, погода и настройки.".to_string()
    })?;
    launch(application_id)?;
    Ok(ActionResult {
        success: true,
        message: format!("Приложение «{}» открыто.", application.trim()),
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
    launch(ApplicationId::Telegram)?;
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
    fn resolves_only_known_application_aliases() {
        assert_eq!(application_id("Телеграмм"), Some(ApplicationId::Telegram));
        assert_eq!(application_id("WhatsApp"), Some(ApplicationId::WhatsApp));
        assert_eq!(application_id("Погода"), Some(ApplicationId::Weather));
        assert_eq!(
            application_id("калькулятор"),
            Some(ApplicationId::Calculator)
        );
        assert_eq!(application_id("неизвестная программа"), None);
    }

    #[test]
    fn validates_external_message_limits() {
        assert!(validate_message("Любимая", "Я скоро буду").is_ok());
        assert!(validate_message("", "Текст").is_err());
        assert!(validate_message("Контакт", "").is_err());
        assert!(validate_message("Контакт", "строка\nс переносом").is_err());
    }
}
