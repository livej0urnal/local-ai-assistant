use serde::Serialize;
use std::process::Command;

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
        "Приложение отсутствует в безопасном каталоге C-3PO. Доступны Telegram, браузер, почта, файлы, заметки, календарь, калькулятор, погода и настройки.".to_string()
    })?;
    launch(application_id)?;
    Ok(ActionResult {
        success: true,
        message: format!("Приложение «{}» открыто.", application.trim()),
    })
}

#[cfg(target_os = "macos")]
fn read_current_weather() -> Result<WeatherReport, String> {
    // Weather не предоставляет обычный локальный API. Читаем только одну
    // доступную AXDescription текущего местоположения; кликов и изменений нет.
    const SCRIPT: &str = r#"
const weather = Application("Weather");
weather.activate();
delay(1.5);
const process = Application("System Events").processes.byName("Weather");
function attribute(element, name) {
  try { return element.attributes.byName(name).value(); } catch (_) { return null; }
}
function findCurrent(element, depth) {
  if (depth > 30) return null;
  const description = attribute(element, "AXDescription");
  if (description && String(description).includes("градус") &&
      (String(description).includes("ТЕКУЩЕЕ МЕСТО") || String(description).includes("CURRENT LOCATION"))) {
    return String(description);
  }
  let children = [];
  try { children = element.uiElements(); } catch (_) { return null; }
  for (const child of children) {
    const result = findCurrent(child, depth + 1);
    if (result) return result;
  }
  return null;
}
const summary = findCurrent(process.windows[0], 0);
if (!summary) throw new Error("current weather was not found");
console.log(summary);
"#;
    let output = Command::new("/usr/bin/osascript")
        .args(["-l", "JavaScript", "-e", SCRIPT])
        .output()
        .map_err(|error| format!("Не удалось прочитать приложение «Погода»: {error}"))?;
    if !output.status.success() {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if error.contains("not allowed assistive access") || error.contains("-25211") {
            return Err("macOS не разрешила чтение приложения «Погода». Разрешите C-3PO в Системные настройки → Конфиденциальность и безопасность → Универсальный доступ.".to_string());
        }
        return Err(format!(
            "Приложение «Погода» не отдало текущие данные: {error}"
        ));
    }
    let summary = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if summary.is_empty() {
        return Err("Приложение «Погода» вернуло пустую сводку".to_string());
    }
    Ok(WeatherReport {
        source: "macOS Weather".to_string(),
        summary,
    })
}

#[cfg(not(target_os = "macos"))]
fn read_current_weather() -> Result<WeatherReport, String> {
    Err("Чтение системной погоды пока подключено только на macOS. На Windows C-3PO может открыть приложение «Погода».".to_string())
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
    fn resolves_only_known_application_aliases() {
        assert_eq!(application_id("Телеграмм"), Some(ApplicationId::Telegram));
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
