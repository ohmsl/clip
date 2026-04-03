use chrono::Utc;
use serde::Serialize;
use std::{
    collections::VecDeque,
    io::{self, Write},
    sync::{Mutex, OnceLock},
};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct LogEvent {
    pub timestamp: String,
    pub level: LogLevel,
    pub source: String,
    pub message: String,
}

struct LogBroadcaster {
    tx: broadcast::Sender<LogEvent>,
}

static BROADCASTER: OnceLock<LogBroadcaster> = OnceLock::new();
static LOG_BUFFER: OnceLock<Mutex<VecDeque<LogEvent>>> = OnceLock::new();
const MAX_BUFFERED_LOGS: usize = 2000;

pub fn init_logging() {
    let _ = BROADCASTER.get_or_init(|| {
        let (tx, _) = broadcast::channel(2048);
        LogBroadcaster { tx }
    });
}

pub fn subscribe() -> broadcast::Receiver<LogEvent> {
    BROADCASTER
        .get_or_init(|| {
            let (tx, _) = broadcast::channel(2048);
            LogBroadcaster { tx }
        })
        .tx
        .subscribe()
}

pub fn emit(level: LogLevel, source: &str, message: impl Into<String>) {
    let broadcaster = BROADCASTER.get_or_init(|| {
        let (tx, _) = broadcast::channel(2048);
        LogBroadcaster { tx }
    });
    let buffer = LOG_BUFFER.get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_BUFFERED_LOGS)));

    let message = message.into();
    let event = LogEvent {
        timestamp: Utc::now().to_rfc3339(),
        level: level.clone(),
        source: source.to_string(),
        message: message.clone(),
    };

    if let Ok(mut guard) = buffer.lock() {
        guard.push_back(event.clone());
        if guard.len() > MAX_BUFFERED_LOGS {
            let overflow = guard.len() - MAX_BUFFERED_LOGS;
            for _ in 0..overflow {
                guard.pop_front();
            }
        }
    }

    let _ = broadcaster.tx.send(event);

    match level {
        LogLevel::Error | LogLevel::Warning => {
            let _ = writeln!(io::stderr(), "[{}] {}", source, message);
        }
        _ => {
            let _ = writeln!(io::stdout(), "[{}] {}", source, message);
        }
    }
}

pub fn info(source: &str, message: impl Into<String>) {
    emit(LogLevel::Info, source, message);
}

pub fn warn(source: &str, message: impl Into<String>) {
    emit(LogLevel::Warning, source, message);
}

pub fn error(source: &str, message: impl Into<String>) {
    emit(LogLevel::Error, source, message);
}

pub fn debug(source: &str, message: impl Into<String>) {
    emit(LogLevel::Debug, source, message);
}

pub fn recent_logs() -> Vec<LogEvent> {
    LOG_BUFFER
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(MAX_BUFFERED_LOGS)))
        .lock()
        .map(|guard| guard.iter().cloned().collect())
        .unwrap_or_default()
}
