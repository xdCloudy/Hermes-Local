from pathlib import Path

CORE = Path("crates/hermes-core/src/lib.rs")
UI = Path("crates/hermes-ui/src/lib.rs")


def replace_once(text: str, old: str, new: str, label: str) -> str:
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"{label}: expected exactly one match, found {count}")
    return text.replace(old, new, 1)


core = CORE.read_text(encoding="utf-8")
core = replace_once(
    core,
    "use std::{\n    future::Future,\n",
    "use std::{\n    collections::{BTreeMap, VecDeque},\n    future::Future,\n",
    "core collections import",
)

marker = "pub trait ConnectionService: Send + Sync {"
queue_code = r'''#[derive(Clone, Debug, Default, PartialEq)]
pub struct PromptQueueCoordinator {
    sessions: BTreeMap<String, PromptQueueSession>,
    runtime_to_stored: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
struct PromptQueueSession {
    runtime_id: String,
    queued: VecDeque<String>,
    parked: bool,
    busy: bool,
    error: Option<String>,
}

impl PromptQueueCoordinator {
    pub fn bind(&mut self, stored_id: &str, runtime_id: &str, busy: bool) {
        if stored_id.is_empty() || runtime_id.is_empty() {
            return;
        }
        let session = self.sessions.entry(stored_id.to_owned()).or_default();
        if !session.runtime_id.is_empty() && session.runtime_id != runtime_id {
            self.runtime_to_stored.remove(&session.runtime_id);
        }
        runtime_id.clone_into(&mut session.runtime_id);
        session.busy = busy;
        self.runtime_to_stored
            .insert(runtime_id.to_owned(), stored_id.to_owned());
    }

    pub fn enqueue(&mut self, stored_id: &str, text: String) -> usize {
        let text = text.trim().to_owned();
        if stored_id.is_empty() || text.is_empty() {
            return self.count(stored_id);
        }
        let session = self.sessions.entry(stored_id.to_owned()).or_default();
        session.queued.push_back(text);
        session.error = None;
        session.queued.len()
    }

    pub fn items(&self, stored_id: &str) -> Vec<String> {
        self.sessions
            .get(stored_id)
            .map(|session| session.queued.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn count(&self, stored_id: &str) -> usize {
        self.sessions
            .get(stored_id)
            .map_or(0, |session| session.queued.len())
    }

    pub fn remove(&mut self, stored_id: &str, index: usize) -> Option<String> {
        self.sessions
            .get_mut(stored_id)
            .and_then(|session| session.queued.remove(index))
    }

    pub fn clear(&mut self, stored_id: &str) -> usize {
        let Some(session) = self.sessions.get_mut(stored_id) else {
            return 0;
        };
        let removed = session.queued.len();
        session.queued.clear();
        session.error = None;
        removed
    }

    pub fn park(&mut self, stored_id: &str) {
        if let Some(session) = self.sessions.get_mut(stored_id) {
            session.parked = true;
        }
    }

    pub fn resume(&mut self, stored_id: &str) {
        if let Some(session) = self.sessions.get_mut(stored_id) {
            session.parked = false;
            session.error = None;
        }
    }

    pub fn is_parked(&self, stored_id: &str) -> bool {
        self.sessions
            .get(stored_id)
            .is_some_and(|session| session.parked)
    }

    pub fn error(&self, stored_id: &str) -> Option<String> {
        self.sessions
            .get(stored_id)
            .and_then(|session| session.error.clone())
    }

    pub fn mark_busy(&mut self, stored_id: &str, busy: bool) {
        if let Some(session) = self.sessions.get_mut(stored_id) {
            session.busy = busy;
            if busy {
                session.error = None;
            }
        }
    }

    pub fn mark_runtime_busy(&mut self, runtime_id: &str) {
        let Some(stored_id) = self.runtime_to_stored.get(runtime_id).cloned() else {
            return;
        };
        self.mark_busy(&stored_id, true);
    }

    pub fn next_after_completion(
        &mut self,
        runtime_id: &str,
    ) -> Option<(String, String, String)> {
        let stored_id = self.runtime_to_stored.get(runtime_id)?.clone();
        let session = self.sessions.get_mut(&stored_id)?;
        session.busy = false;
        if session.parked {
            return None;
        }
        let text = session.queued.pop_front()?;
        session.busy = true;
        session.error = None;
        Some((stored_id, runtime_id.to_owned(), text))
    }

    pub fn next_if_idle(&mut self, stored_id: &str) -> Option<(String, String)> {
        let session = self.sessions.get_mut(stored_id)?;
        if session.busy || session.parked || session.runtime_id.is_empty() {
            return None;
        }
        let text = session.queued.pop_front()?;
        session.busy = true;
        session.error = None;
        Some((session.runtime_id.clone(), text))
    }

    pub fn mark_submit_failed(&mut self, stored_id: &str, text: String, error: String) {
        let session = self.sessions.entry(stored_id.to_owned()).or_default();
        session.busy = false;
        session.queued.push_front(text);
        session.error = Some(error);
    }
}

#[cfg(test)]
mod prompt_queue_tests {
    use super::PromptQueueCoordinator;

    #[test]
    fn queues_are_fifo_and_isolated_across_background_sessions() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored-a", "runtime-a", true);
        queue.bind("stored-b", "runtime-b", true);
        queue.enqueue("stored-a", "a1".into());
        queue.enqueue("stored-a", "a2".into());
        queue.enqueue("stored-b", "b1".into());

        assert_eq!(
            queue.next_after_completion("runtime-b"),
            Some(("stored-b".into(), "runtime-b".into(), "b1".into()))
        );
        assert_eq!(
            queue.next_after_completion("runtime-a"),
            Some(("stored-a".into(), "runtime-a".into(), "a1".into()))
        );
        assert_eq!(queue.count("stored-a"), 1);
        assert_eq!(queue.count("stored-b"), 0);
        assert_eq!(
            queue.next_after_completion("runtime-a"),
            Some(("stored-a".into(), "runtime-a".into(), "a2".into()))
        );
    }

    #[test]
    fn stop_parks_queue_until_explicit_resume() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored", "runtime", true);
        queue.enqueue("stored", "later".into());
        queue.park("stored");

        assert_eq!(queue.next_after_completion("runtime"), None);
        assert!(queue.is_parked("stored"));
        assert_eq!(queue.count("stored"), 1);

        queue.resume("stored");
        assert_eq!(
            queue.next_if_idle("stored"),
            Some(("runtime".into(), "later".into()))
        );
    }

    #[test]
    fn queued_prompts_can_be_cancelled_without_touching_other_sessions() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("a", "ra", true);
        queue.bind("b", "rb", true);
        queue.enqueue("a", "one".into());
        queue.enqueue("a", "two".into());
        queue.enqueue("b", "other".into());

        assert_eq!(queue.remove("a", 0), Some("one".into()));
        assert_eq!(queue.items("a"), vec!["two".to_owned()]);
        assert_eq!(queue.clear("a"), 1);
        assert!(queue.items("a").is_empty());
        assert_eq!(queue.items("b"), vec!["other".to_owned()]);
    }

    #[test]
    fn failed_background_submission_returns_prompt_to_front() {
        let mut queue = PromptQueueCoordinator::default();
        queue.bind("stored", "runtime", false);
        queue.enqueue("stored", "first".into());
        queue.enqueue("stored", "second".into());
        let (_, first) = queue.next_if_idle("stored").expect("first queued prompt");
        queue.mark_submit_failed("stored", first, "offline".into());

        assert_eq!(queue.items("stored"), vec!["first", "second"]);
        assert_eq!(queue.error("stored").as_deref(), Some("offline"));
    }
}

''' + marker
core = replace_once(core, marker, queue_code, "queue coordinator insertion")
CORE.write_text(core, encoding="utf-8")

ui = UI.read_text(encoding="utf-8")
ui = replace_once(
    ui,
    "use chat::{Chat, Session};",
    "use chat::{Chat, ChatRuntimeProvider, Session};",
    "chat runtime import",
)
ui = replace_once(
    ui,
    "            Router::<Route> {}",
    "            ChatRuntimeProvider {}",
    "root router replacement",
)
UI.write_text(ui, encoding="utf-8")
