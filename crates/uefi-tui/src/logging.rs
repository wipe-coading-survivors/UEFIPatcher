use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use uefi_common::state::state_dir;

fn log_path() -> PathBuf {
    state_dir().join("tui.log")
}

fn open_log_file(path: &Path) -> Option<File> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent).ok()?;
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn append_line(path: &Path, line: &str) -> io::Result<()> {
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(f, "{line}")
}

fn format_panic(msg: &str, loc: &str) -> String {
    format!("PANIC at {loc}: {msg}")
}

fn panic_message(info: &std::panic::PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// Устанавливает глобальный fmt-подписчик, пишущий в `<state_dir>/tui.log`
/// (append, без ANSI; фильтр `RUST_LOG` либо умолчание `uefi_tui=info,warn`),
/// и возвращает путь к файлу. НЕ ротирует и не очищает лог. При ошибке
/// создания файла/директории возвращает `None` и ничего не устанавливает —
/// TUI продолжает работать без лога.
pub fn init() -> Option<PathBuf> {
    let path = log_path();
    let file = open_log_file(&path)?;
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("uefi_tui=info,warn"));
    tracing_subscriber::fmt()
        .with_writer(Mutex::new(file))
        .with_ansi(false)
        .with_env_filter(filter)
        .init();
    Some(path)
}

/// Устанавливает panic-hook: сообщение и location дописываются в `log_path`
/// напрямую (в обход tracing-подписчика — он мог стать источником паники),
/// затем восстанавливается терминал (raw mode off + LeaveAlternateScreen) и
/// паника печатается в stderr как у дефолтного хука. Ошибки записи в лог и
/// восстановления терминала глушатся — хук сам паниковать не должен.
pub fn install_panic_hook(log_path: PathBuf) {
    std::panic::set_hook(Box::new(move |info| {
        let msg = panic_message(info);
        let loc = info
            .location()
            .map(|l| l.to_string())
            .unwrap_or_else(|| "<unknown>".into());
        let _ = append_line(&log_path, &format_panic(&msg, &loc));
        let _ = crossterm::terminal::disable_raw_mode();
        let _ = crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen);
        eprintln!(
            "\nuefi-tui panicked at {loc}:\n  {msg}\nlog: {}",
            log_path.display()
        );
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn open_log_file_creates_missing_file() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("tui.log");
        assert!(open_log_file(&p).is_some());
        assert!(p.exists());
    }

    #[test]
    fn open_log_file_appends_preserving_content() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("tui.log");
        {
            let mut f = open_log_file(&p).unwrap();
            writeln!(f, "first").unwrap();
        }
        {
            let mut f = open_log_file(&p).unwrap();
            writeln!(f, "second").unwrap();
        }
        let s = std::fs::read_to_string(&p).unwrap();
        assert_eq!(s, "first\nsecond\n");
    }

    #[test]
    fn open_log_file_creates_nested_parents() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("a").join("b").join("tui.log");
        assert!(open_log_file(&p).is_some());
        assert!(p.exists());
    }

    #[test]
    fn open_log_file_returns_none_when_parent_is_file() {
        let td = TempDir::new().unwrap();
        let blocker = td.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let p = blocker.join("tui.log");
        assert!(open_log_file(&p).is_none());
    }

    #[test]
    fn append_line_appends_lines() {
        let td = TempDir::new().unwrap();
        let p = td.path().join("panic.log");
        append_line(&p, "one").unwrap();
        append_line(&p, "two").unwrap();
        let s = std::fs::read_to_string(&p).unwrap();
        assert_eq!(s, "one\ntwo\n");
    }

    #[test]
    fn format_panic_contains_message_and_location() {
        let s = format_panic("boom", "src/main.rs:42:9");
        assert!(s.contains("boom"), "got: {s}");
        assert!(s.contains("src/main.rs:42:9"), "got: {s}");
    }

    #[test]
    fn log_path_under_state_dir() {
        let p = log_path();
        assert!(p.starts_with(state_dir()));
        assert!(p.ends_with("tui.log"));
    }
}
