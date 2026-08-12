from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one TM-01 DSR marker, got {count}: {old[:140]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


path = "crates/hermes-desktop/src/lib.rs"
replace_once(
    path,
    """struct TerminalProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<Vec<u8>>>,
}""",
    """struct TerminalProcess {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    output: Arc<Mutex<Vec<u8>>>,
    control_tail: Vec<u8>,
}""",
)
replace_once(
    path,
    """                        child,
                        output,
                    },""",
    """                        child,
                        output,
                        control_tail: Vec::new(),
                    },""",
)
replace_once(
    path,
    """            let processes = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?;
            let process = processes
                .get(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            let mut output = process
                .output
                .lock()
                .map_err(|_| ServiceError::Platform("terminal output lock was poisoned".into()))?;
            Ok(std::mem::take(&mut *output))""",
    """            let mut processes = self
                .processes
                .lock()
                .map_err(|_| ServiceError::Platform("terminal lock was poisoned".into()))?;
            let process = processes
                .get_mut(&id)
                .ok_or_else(|| ServiceError::NotFound("terminal".into()))?;
            let bytes = {
                let mut output = process.output.lock().map_err(|_| {
                    ServiceError::Platform("terminal output lock was poisoned".into())
                })?;
                std::mem::take(&mut *output)
            };

            let mut control_window = Vec::with_capacity(process.control_tail.len() + bytes.len());
            control_window.extend_from_slice(&process.control_tail);
            control_window.extend_from_slice(&bytes);
            let cursor_queries = control_window
                .windows(4)
                .filter(|window| *window == b"\\x1b[6n")
                .count();
            let tail_start = control_window.len().saturating_sub(3);
            process.control_tail.clear();
            process
                .control_tail
                .extend_from_slice(&control_window[tail_start..]);

            if cursor_queries > 0 {
                for _ in 0..cursor_queries {
                    process.writer.write_all(b"\\x1b[1;1R").map_err(platform)?;
                }
                process.writer.flush().map_err(platform)?;
            }
            Ok(bytes)""",
)
