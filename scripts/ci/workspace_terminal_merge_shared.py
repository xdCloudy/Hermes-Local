from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    p = Path(path)
    text = p.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{path}: expected one TM-01 merge marker, got {count}: {old[:140]!r}")
    p.write_text(text.replace(old, new), encoding="utf-8")


core = "crates/hermes-core/src/lib.rs"
replace_once(
    core,
    """pub trait TerminalService: Send + Sync {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String>;
    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()>;
    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()>;
    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()>;
}""",
    """pub trait TerminalService: Send + Sync {
    fn start(&self, cwd: &Path, cols: u16, rows: u16) -> ServiceFuture<'_, String>;
    fn write(&self, id: &str, data: &[u8]) -> ServiceFuture<'_, ()>;
    fn read(&self, id: &str) -> ServiceFuture<'_, Vec<u8>>;
    fn resize(&self, id: &str, cols: u16, rows: u16) -> ServiceFuture<'_, ()>;
    fn dispose(&self, id: &str) -> ServiceFuture<'_, ()>;
    fn dispose_now(&self, _id: &str) -> ServiceResult<()> {
        Err(ServiceError::Unavailable(
            "synchronous terminal disposal is unavailable on this platform".into(),
        ))
    }
}""",
)

ui = "crates/hermes-ui/src/lib.rs"
replace_once(
    ui,
    """mod files;
mod review;
mod source_control;
use files::Files;
use review::Review;
use source_control::{Git, Worktrees};""",
    """mod files;
mod review;
mod source_control;
mod terminal;
use files::Files;
use review::Review;
use source_control::{Git, Worktrees};
use terminal::Terminal;""",
)
replace_once(
    ui,
    """simple_surface!(
    Terminal,
    "Developer tools",
    "Terminal",
    "A native ConPTY session scoped to your workspace."
);
""",
    "",
)
