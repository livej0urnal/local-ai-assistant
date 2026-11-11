use serde::{Deserialize, Serialize};
use std::process::Command;

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MailMessageSummary {
    sender: String,
    subject: String,
    received_at: String,
    is_read: bool,
    is_flagged: bool,
    preview: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MailOverview {
    source: String,
    messages: Vec<MailMessageSummary>,
    unread: usize,
    flagged: usize,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CalendarEventSummary {
    calendar: String,
    title: String,
    start_at: String,
    end_at: String,
    location: String,
    all_day: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CalendarOverview {
    source: String,
    range_days: u8,
    events: Vec<CalendarEventSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductivityActionResult {
    success: bool,
    message: String,
    undo_kind: Option<String>,
    undo_token: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProductivityPermissionReport {
    mail: bool,
    calendar: bool,
    message: String,
}

/// Просит macOS показать штатные окна Automation, не читая письма и события.
#[cfg(target_os = "macos")]
fn prime_productivity_permissions_native() -> ProductivityPermissionReport {
    let mail = run_jxa("function run() { return Application('Mail').name(); }", &[]).is_ok();
    let calendar = run_jxa(
        "function run() { return Application('Calendar').name(); }",
        &[],
    )
    .is_ok();
    ProductivityPermissionReport {
        mail,
        calendar,
        message: if mail && calendar {
            "Доступ к Почте и Календарю готов.".to_string()
        } else {
            "Разрешите C-3PO управлять Почтой и Календарём в системном окне.".to_string()
        },
    }
}

#[cfg(not(target_os = "macos"))]
fn prime_productivity_permissions_native() -> ProductivityPermissionReport {
    ProductivityPermissionReport {
        mail: false,
        calendar: false,
        message: "На Windows почта и календарь подключаются через Microsoft 365.".to_string(),
    }
}

#[tauri::command]
pub(crate) async fn prime_productivity_permissions() -> ProductivityPermissionReport {
    tauri::async_runtime::spawn_blocking(prime_productivity_permissions_native)
        .await
        .unwrap_or_else(|error| ProductivityPermissionReport {
            mail: false,
            calendar: false,
            message: format!("Проверка разрешений остановилась: {error}"),
        })
}

#[cfg(target_os = "macos")]
fn run_jxa(script: &str, arguments: &[&str]) -> Result<String, String> {
    let mut command = Command::new("/usr/bin/osascript");
    command.args(["-l", "JavaScript", "-e", script]);
    command.args(arguments);
    let output = command
        .output()
        .map_err(|error| format!("Не удалось запустить системный адаптер: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let details = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(
            if details.contains("-1743") || details.contains("not authorized") {
                "macOS не разрешила C-3PO доступ к приложению. Откройте Системные настройки → Конфиденциальность и безопасность → Автоматизация и разрешите доступ.".to_string()
            } else {
                format!("Системный адаптер вернул ошибку: {details}")
            },
        )
    }
}

#[cfg(target_os = "macos")]
fn read_mail_overview(limit: u8) -> Result<MailOverview, String> {
    const SCRIPT: &str = r#"
function run(argv) {
  const limit = Math.max(1, Math.min(50, Number(argv[0]) || 20));
  const Mail = Application('Mail');
  const inbox = Mail.inbox();
  const source = inbox.messages();
  const result = [];
  for (let index = 0; index < Math.min(limit, source.length); index += 1) {
    const message = source[index];
    let content = '';
    try { content = String(message.content() || '').replace(/\s+/g, ' ').trim().slice(0, 500); } catch (_) {}
    result.push({
      sender: String(message.sender() || '').slice(0, 300),
      subject: String(message.subject() || '(без темы)').slice(0, 300),
      receivedAt: new Date(message.dateReceived()).toISOString(),
      isRead: Boolean(message.readStatus()),
      isFlagged: Boolean(message.flaggedStatus()),
      preview: content
    });
  }
  return JSON.stringify(result);
}
"#;
    let payload = run_jxa(SCRIPT, &[&limit.to_string()])?;
    let messages: Vec<MailMessageSummary> = serde_json::from_str(&payload)
        .map_err(|error| format!("Не удалось разобрать локальную сводку почты: {error}"))?;
    let unread = messages.iter().filter(|message| !message.is_read).count();
    let flagged = messages.iter().filter(|message| message.is_flagged).count();
    Ok(MailOverview {
        source: "Apple Mail на этом Mac".to_string(),
        messages,
        unread,
        flagged,
    })
}

#[cfg(not(target_os = "macos"))]
fn read_mail_overview(_limit: u8) -> Result<MailOverview, String> {
    Err("Для Windows требуется подключение Microsoft 365 через OAuth. Локальный адаптер пока доступен только для Apple Mail.".to_string())
}

#[tauri::command]
pub(crate) async fn get_mail_overview(limit: Option<u8>) -> Result<MailOverview, String> {
    let limit = limit.unwrap_or(20).clamp(1, 50);
    tauri::async_runtime::spawn_blocking(move || read_mail_overview(limit))
        .await
        .map_err(|error| format!("Анализ почты неожиданно остановился: {error}"))?
}

#[cfg(target_os = "macos")]
fn read_calendar_overview(days: u8) -> Result<CalendarOverview, String> {
    const SCRIPT: &str = r#"
function run(argv) {
  const days = Math.max(1, Math.min(30, Number(argv[0]) || 7));
  const Calendar = Application('Calendar');
  const start = new Date();
  const end = new Date(start.getTime() + days * 86400000);
  const result = [];
  for (const calendar of Calendar.calendars()) {
    let events = [];
    try { events = calendar.events.whose({startDate: {_greaterThan: start}})(); } catch (_) { continue; }
    for (const event of events) {
      const eventStart = new Date(event.startDate());
      if (eventStart > end || result.length >= 200) continue;
      result.push({
        calendar: String(calendar.name() || ''),
        title: String(event.summary() || '(без названия)').slice(0, 300),
        startAt: eventStart.toISOString(),
        endAt: new Date(event.endDate()).toISOString(),
        location: String(event.location() || '').slice(0, 300),
        allDay: Boolean(event.alldayEvent())
      });
    }
  }
  result.sort((left, right) => left.startAt.localeCompare(right.startAt));
  return JSON.stringify(result);
}
"#;
    let payload = run_jxa(SCRIPT, &[&days.to_string()])?;
    let events: Vec<CalendarEventSummary> = serde_json::from_str(&payload)
        .map_err(|error| format!("Не удалось разобрать локальную сводку календаря: {error}"))?;
    Ok(CalendarOverview {
        source: "Календарь на этом Mac".to_string(),
        range_days: days,
        events,
    })
}

#[cfg(not(target_os = "macos"))]
fn read_calendar_overview(_days: u8) -> Result<CalendarOverview, String> {
    Err("Для Windows требуется подключение Microsoft 365 через OAuth. Локальный адаптер пока доступен только для Календаря macOS.".to_string())
}

#[tauri::command]
pub(crate) async fn get_calendar_overview(days: Option<u8>) -> Result<CalendarOverview, String> {
    let days = days.unwrap_or(7).clamp(1, 30);
    tauri::async_runtime::spawn_blocking(move || read_calendar_overview(days))
        .await
        .map_err(|error| format!("Анализ календаря неожиданно остановился: {error}"))?
}

fn validate_draft(recipient: &str, subject: &str, body: &str) -> Result<(), String> {
    if recipient.is_empty()
        || recipient.chars().count() > 254
        || !recipient.contains('@')
        || recipient.chars().any(char::is_whitespace)
    {
        return Err("Укажите корректный адрес получателя".to_string());
    }
    if subject.is_empty() || subject.chars().count() > 300 || subject.chars().any(char::is_control)
    {
        return Err("Тема должна содержать от 1 до 300 символов".to_string());
    }
    if body.is_empty() || body.chars().count() > 20_000 {
        return Err("Текст письма должен содержать от 1 до 20 000 символов".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn create_mail_draft_native(recipient: &str, subject: &str, body: &str) -> Result<String, String> {
    const SCRIPT: &str = r#"
function run(argv) {
  const Mail = Application('Mail');
  const draft = Mail.OutgoingMessage({subject: argv[1], content: argv[2] + '\n', visible: true});
  Mail.outgoingMessages.push(draft);
  draft.toRecipients.push(Mail.Recipient({address: argv[0]}));
  Mail.activate();
  return String(draft.id());
}
"#;
    run_jxa(SCRIPT, &[recipient, subject, body])
}

#[cfg(not(target_os = "macos"))]
fn create_mail_draft_native(
    _recipient: &str,
    _subject: &str,
    _body: &str,
) -> Result<String, String> {
    Err("Создание почтового черновика на Windows будет доступно после подключения Microsoft 365 через OAuth.".to_string())
}

#[tauri::command]
pub(crate) fn create_mail_draft(
    recipient: String,
    subject: String,
    body: String,
) -> Result<ProductivityActionResult, String> {
    let recipient = recipient.trim();
    let subject = subject.trim();
    let body = body.trim();
    validate_draft(recipient, subject, body)?;
    let undo_token = create_mail_draft_native(recipient, subject, body).map_err(|error| {
        crate::executor::audit_action(
            "email-draft",
            "medium",
            "failed",
            serde_json::json!({
                "recipientCharacters": recipient.chars().count(),
                "subjectCharacters": subject.chars().count(),
                "bodyCharacters": body.chars().count(),
            }),
        );
        error
    })?;
    crate::executor::audit_action(
        "email-draft",
        "medium",
        "completed",
        serde_json::json!({
            "recipientCharacters": recipient.chars().count(),
            "subjectCharacters": subject.chars().count(),
            "bodyCharacters": body.chars().count(),
        }),
    );
    Ok(ProductivityActionResult {
        success: true,
        message: format!(
            "Черновик письма для «{recipient}» открыт в Apple Mail. Отправьте его после проверки."
        ),
        undo_kind: Some("mail-draft".to_string()),
        undo_token: Some(undo_token),
    })
}

fn validate_calendar_event(
    title: &str,
    start_at: &str,
    end_at: &str,
    notes: &str,
) -> Result<(), String> {
    if title.is_empty() || title.chars().count() > 300 || title.chars().any(char::is_control) {
        return Err("Название события должно содержать от 1 до 300 символов".to_string());
    }
    if start_at.len() > 64
        || end_at.len() > 64
        || !start_at.contains('T')
        || !end_at.contains('T')
        || start_at.chars().any(char::is_whitespace)
        || end_at.chars().any(char::is_whitespace)
    {
        return Err("Некорректные дата или время события".to_string());
    }
    if notes.chars().count() > 10_000 {
        return Err("Заметка события не должна превышать 10 000 символов".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn create_calendar_event_native(
    title: &str,
    start_at: &str,
    end_at: &str,
    notes: &str,
) -> Result<String, String> {
    const SCRIPT: &str = r#"
function run(argv) {
  const Calendar = Application('Calendar');
  const start = new Date(argv[1]);
  const end = new Date(argv[2]);
  if (!Number.isFinite(start.getTime()) || !Number.isFinite(end.getTime()) || end <= start) {
    throw new Error('Дата окончания должна быть позже даты начала');
  }
  const calendars = Calendar.calendars();
  let target = null;
  for (const calendar of calendars) {
    try { if (calendar.writable()) { target = calendar; break; } } catch (_) {}
  }
  if (!target && calendars.length) target = calendars[0];
  if (!target) throw new Error('Не найден доступный календарь');
  const event = Calendar.Event({summary: argv[0], startDate: start, endDate: end, description: argv[3] || ''});
  target.events.push(event);
  Calendar.activate();
  return String(event.uid());
}
"#;
    run_jxa(SCRIPT, &[title, start_at, end_at, notes])
}

#[cfg(not(target_os = "macos"))]
fn create_calendar_event_native(
    _title: &str,
    _start_at: &str,
    _end_at: &str,
    _notes: &str,
) -> Result<String, String> {
    Err(
        "Создание события на Windows будет доступно после подключения Microsoft 365 через OAuth."
            .to_string(),
    )
}

#[tauri::command]
pub(crate) fn create_calendar_event(
    title: String,
    start_at: String,
    end_at: String,
    notes: String,
) -> Result<ProductivityActionResult, String> {
    let title = title.trim();
    let start_at = start_at.trim();
    let end_at = end_at.trim();
    let notes = notes.trim();
    validate_calendar_event(title, start_at, end_at, notes)?;
    let undo_token = create_calendar_event_native(title, start_at, end_at, notes).map_err(|error| {
        crate::executor::audit_action(
            "calendar-event",
            "medium",
            "failed",
            serde_json::json!({ "titleCharacters": title.chars().count(), "hasNotes": !notes.is_empty() }),
        );
        error
    })?;
    crate::executor::audit_action(
        "calendar-event",
        "medium",
        "completed",
        serde_json::json!({ "titleCharacters": title.chars().count(), "hasNotes": !notes.is_empty() }),
    );
    Ok(ProductivityActionResult {
        success: true,
        message: format!("Событие «{title}» добавлено в календарь."),
        undo_kind: Some("calendar-event".to_string()),
        undo_token: Some(undo_token),
    })
}

fn validate_undo_token(token: &str) -> Result<(), String> {
    if token.is_empty() || token.chars().count() > 512 || token.chars().any(char::is_control) {
        return Err("Некорректная запись отмены".to_string());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn undo_productivity_action_native(kind: &str, token: &str) -> Result<(), String> {
    const MAIL_SCRIPT: &str = r#"
function run(argv) {
  const Mail = Application('Mail');
  for (const draft of Mail.outgoingMessages()) {
    if (String(draft.id()) === argv[0]) { Mail.delete(draft); return 'ok'; }
  }
  throw new Error('Черновик уже закрыт, отправлен или удалён');
}
"#;
    const CALENDAR_SCRIPT: &str = r#"
function run(argv) {
  const Calendar = Application('Calendar');
  for (const calendar of Calendar.calendars()) {
    for (const event of calendar.events()) {
      try { if (String(event.uid()) === argv[0]) { Calendar.delete(event); return 'ok'; } } catch (_) {}
    }
  }
  throw new Error('Событие уже изменено или удалено');
}
"#;
    match kind {
        "mail-draft" => run_jxa(MAIL_SCRIPT, &[token]).map(|_| ()),
        "calendar-event" => run_jxa(CALENDAR_SCRIPT, &[token]).map(|_| ()),
        _ => Err("Этот тип действия нельзя отменить".to_string()),
    }
}

#[cfg(not(target_os = "macos"))]
fn undo_productivity_action_native(_kind: &str, _token: &str) -> Result<(), String> {
    Err("Отмена этого действия пока доступна только на macOS".to_string())
}

#[tauri::command]
pub(crate) fn undo_productivity_action(
    kind: String,
    token: String,
) -> Result<ProductivityActionResult, String> {
    validate_undo_token(&token)?;
    undo_productivity_action_native(&kind, &token)?;
    crate::executor::audit_action(
        "undo-productivity-action",
        "low",
        "completed",
        serde_json::json!({ "kind": kind }),
    );
    Ok(ProductivityActionResult {
        success: true,
        message: "Последнее действие отменено.".to_string(),
        undo_kind: None,
        undo_token: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_mail_draft_without_accepting_commands_as_addresses() {
        assert!(validate_draft("user@example.com", "Встреча", "Добрый день").is_ok());
        assert!(validate_draft("open calculator", "Встреча", "Текст").is_err());
        assert!(validate_draft("user@example.com", "", "Текст").is_err());
        assert!(validate_draft("user@example.com", "Тема", "").is_err());
    }

    #[test]
    fn validates_calendar_event_shape() {
        assert!(
            validate_calendar_event(
                "Встреча",
                "2026-11-12T09:00:00.000Z",
                "2026-11-12T10:00:00.000Z",
                ""
            )
            .is_ok()
        );
        assert!(validate_calendar_event("", "bad", "bad", "").is_err());
        assert!(validate_calendar_event("Встреча", "tomorrow", "later", "").is_err());
    }
}
