use std::{
    cell::RefCell,
    ptr::NonNull,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use block2::RcBlock;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_avf_audio::{AVAudioEngine, AVAudioInputNode, AVAudioPCMBuffer, AVAudioTime};
use objc2_foundation::{NSArray, NSError, NSLocale, NSString};
use objc2_speech::{
    SFSpeechAudioBufferRecognitionRequest, SFSpeechRecognitionResult, SFSpeechRecognitionTask,
    SFSpeechRecognizer, SFSpeechRecognizerAuthorizationStatus,
};
use tauri::{AppHandle, Emitter};

use crate::{append_native_log, reveal_main_window};

type TapBlock = RcBlock<dyn Fn(NonNull<AVAudioPCMBuffer>, NonNull<AVAudioTime>)>;
type ResultBlock = RcBlock<dyn Fn(*mut SFSpeechRecognitionResult, *mut NSError)>;

struct WakeRuntime {
    engine: Retained<AVAudioEngine>,
    input: Retained<AVAudioInputNode>,
    request: Retained<SFSpeechAudioBufferRecognitionRequest>,
    task: Retained<SFSpeechRecognitionTask>,
    _recognizer: Retained<SFSpeechRecognizer>,
    _tap_block: TapBlock,
    _result_block: ResultBlock,
}

impl Drop for WakeRuntime {
    fn drop(&mut self) {
        // SAFETY: Объекты создаются и заменяются только в главном потоке macOS.
        unsafe {
            self.input.removeTapOnBus(0);
            self.engine.stop();
            self.request.endAudio();
            self.task.cancel();
        }
    }
}

thread_local! {
    static RUNTIME: RefCell<Option<WakeRuntime>> = const { RefCell::new(None) };
}

static RESTART_TIMER_STARTED: AtomicBool = AtomicBool::new(false);

fn contains_wake_phrase(text: &str) -> bool {
    let normalized = text
        .to_lowercase()
        .replace(['-', '—', ',', '.', '!', '?'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    [
        "c 3 p o",
        "c 3po",
        "c3po",
        "си три пи о",
        "с три пи о",
        "с 3 по",
        "ситрипио",
    ]
    .iter()
    .any(|phrase| normalized.contains(phrase))
}

fn restart_later(app: AppHandle, seconds: u64) {
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(seconds));
        let main_app = app.clone();
        let _ = app.run_on_main_thread(move || restart_session(main_app));
    });
}

fn start_session(app: AppHandle) -> Result<(), String> {
    RUNTIME.with(|runtime| runtime.borrow_mut().take());
    let locale = NSLocale::localeWithLocaleIdentifier(&NSString::from_str("ru-RU"));
    // SAFETY: Инициализация Apple Speech выполняется в главном потоке.
    let recognizer =
        unsafe { SFSpeechRecognizer::initWithLocale(SFSpeechRecognizer::alloc(), &locale) }
            .ok_or_else(|| "macOS не создала распознаватель ru-RU".to_string())?;
    if !unsafe { recognizer.isAvailable() } {
        return Err("служба распознавания речи macOS недоступна".to_string());
    }
    // Принципиальное ограничение приватности: сетевое распознавание Apple не
    // используется. Если локальная модель macOS недоступна, слушатель выключен.
    if !unsafe { recognizer.supportsOnDeviceRecognition() } {
        return Err("локальное распознавание ru-RU не поддерживается этой macOS".to_string());
    }

    let request = unsafe { SFSpeechAudioBufferRecognitionRequest::new() };
    unsafe {
        request.setShouldReportPartialResults(true);
        request.setRequiresOnDeviceRecognition(true);
    }
    let phrases = [
        NSString::from_str("C-3PO"),
        NSString::from_str("C3PO"),
        NSString::from_str("Си три пи о"),
    ];
    unsafe { request.setContextualStrings(&NSArray::from_retained_slice(&phrases)) };

    let triggered = Arc::new(AtomicBool::new(false));
    let callback_app = app.clone();
    let callback_triggered = Arc::clone(&triggered);
    let result_block: ResultBlock = RcBlock::new(
        move |result: *mut SFSpeechRecognitionResult, error: *mut NSError| {
            if !error.is_null() || result.is_null() {
                return;
            }
            // SAFETY: result действителен на протяжении вызова блока Apple.
            let text = unsafe { (&*result).bestTranscription().formattedString().to_string() };
            let _ = callback_app.emit("native-speech-transcript", text.clone());
            if !contains_wake_phrase(&text) || callback_triggered.swap(true, Ordering::SeqCst) {
                return;
            }
            append_native_log(
                "info",
                "voice.native-wake-word",
                &format!("Распознано: {text}"),
            );
            reveal_main_window(&callback_app);
            let _ = callback_app.emit("native-wake-word", text);
            restart_later(callback_app.clone(), 2);
        },
    );
    let task =
        unsafe { recognizer.recognitionTaskWithRequest_resultHandler(&request, &result_block) };

    let engine = unsafe { AVAudioEngine::init(AVAudioEngine::alloc()) };
    let input = unsafe { engine.inputNode() };
    let format = unsafe { input.outputFormatForBus(0) };
    let audio_request = request.clone();
    let tap_block: TapBlock = RcBlock::new(
        move |buffer: NonNull<AVAudioPCMBuffer>, _time: NonNull<AVAudioTime>| {
            // SAFETY: AVFoundation гарантирует жизнь буфера на время callback.
            unsafe { audio_request.appendAudioPCMBuffer(buffer.as_ref()) };
        },
    );
    unsafe {
        input.installTapOnBus_bufferSize_format_block(
            0,
            2_048,
            Some(&format),
            RcBlock::as_ptr(&tap_block),
        );
        engine.prepare();
        engine
            .startAndReturnError()
            .map_err(|error| error.localizedDescription().to_string())?;
    }

    RUNTIME.with(|runtime| {
        *runtime.borrow_mut() = Some(WakeRuntime {
            engine,
            input,
            request,
            task,
            _recognizer: recognizer,
            _tap_block: tap_block,
            _result_block: result_block,
        });
    });
    append_native_log(
        "info",
        "voice.native-wake-word",
        "Локальный слушатель macOS запущен",
    );
    Ok(())
}

fn restart_session(app: AppHandle) {
    if let Err(error) = start_session(app) {
        append_native_log("warn", "voice.native-wake-word", &error);
    }
}

fn start_restart_timer(app: AppHandle) {
    if RESTART_TIMER_STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    thread::spawn(move || {
        loop {
            thread::sleep(Duration::from_secs(50));
            let main_app = app.clone();
            if app
                .run_on_main_thread(move || restart_session(main_app))
                .is_err()
            {
                break;
            }
        }
    });
}

pub(crate) fn start_native_wake_listener(app: AppHandle) {
    // SAFETY: Читается системное состояние разрешения Speech framework.
    let status = unsafe { SFSpeechRecognizer::authorizationStatus() };
    if status == SFSpeechRecognizerAuthorizationStatus::Authorized {
        restart_session(app.clone());
        start_restart_timer(app);
        return;
    }
    if status != SFSpeechRecognizerAuthorizationStatus::NotDetermined {
        append_native_log(
            "warn",
            "voice.native-wake-word",
            "Нет разрешения на распознавание речи",
        );
        return;
    }

    let authorization_app = app.clone();
    let authorization_block = RcBlock::new(move |new_status| {
        let authorized = new_status == SFSpeechRecognizerAuthorizationStatus::Authorized;
        let main_app = authorization_app.clone();
        let _ = authorization_app.run_on_main_thread(move || {
            if authorized {
                restart_session(main_app.clone());
                start_restart_timer(main_app);
            } else {
                append_native_log(
                    "warn",
                    "voice.native-wake-word",
                    "Разрешение не предоставлено",
                );
            }
        });
    });
    unsafe { SFSpeechRecognizer::requestAuthorization(&authorization_block) };
}

pub(crate) fn restart_native_wake_listener(app: AppHandle) {
    let main_app = app.clone();
    let _ = app.run_on_main_thread(move || restart_session(main_app));
}

#[cfg(test)]
mod tests {
    use super::contains_wake_phrase;

    #[test]
    fn recognises_supported_wake_phrases() {
        assert!(contains_wake_phrase("Привет, C-3PO"));
        assert!(contains_wake_phrase("си три пи о"));
        assert!(contains_wake_phrase("с 3 по"));
        assert!(!contains_wake_phrase("обычный разговор"));
    }
}
