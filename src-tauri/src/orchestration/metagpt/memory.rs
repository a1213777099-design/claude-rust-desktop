use super::message::{CauseBy, Message};
use std::collections::HashMap;

pub struct Memory {
    storage: Vec<Message>,
    index: HashMap<String, Vec<usize>>,
}

impl Memory {
    pub fn new() -> Self { Self { storage: Vec::new(), index: HashMap::new() } }

    pub fn add(&mut self, message: Message) {
        if self.storage.iter().any(|m| m.id == message.id) { return; }
        let idx = self.storage.len();
        let key = message.cause_by.as_str().to_string();
        self.index.entry(key).or_default().push(idx);
        self.storage.push(message);
    }

    pub fn add_batch(&mut self, msgs: Vec<Message>) { for m in msgs { self.add(m); } }

    pub fn get_by_cause(&self, cause_by: &CauseBy) -> Vec<&Message> {
        self.index.get(cause_by.as_str())
            .map(|idx| idx.iter().filter_map(|&i| self.storage.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn find_news(&self, observed: &[Message]) -> Vec<Message> {
        observed.iter().filter(|o| !self.storage.iter().any(|s| s.id == o.id)).cloned().collect()
    }

    pub fn get(&self, k: usize) -> Vec<&Message> {
        let start = if k == 0 { 0 } else { self.storage.len().saturating_sub(k) };
        self.storage[start..].iter().collect()
    }

    pub fn get_all(&self) -> &[Message] { &self.storage }

    pub fn count(&self) -> usize { self.storage.len() }
    pub fn clear(&mut self) { self.storage.clear(); self.index.clear(); }

    pub fn get_relevant(&self, max_chars: usize) -> String {
        let mut ctx = String::new();
        for msg in self.storage.iter().rev() {
            let line = format!("[{}] {}: {}
", msg.cause_by.as_str(), msg.sent_from, msg.content);
            if ctx.len() + line.len() > max_chars { break; }
            ctx.insert_str(0, &line);
        }
        ctx
    }

    /// Persist all memory messages to SQLite for cross-workflow continuity.
    pub fn persist_to(&self, conn: &rusqlite::Connection, workspace: &str) -> anyhow::Result<()> {
        use crate::db::memory_repo;
        let now = chrono::Utc::now().to_rfc3339();
        for msg in &self.storage {
            let id = &msg.id;
            let summary: String = msg.content.chars().take(2000).collect();
            let tags = format!("metagpt_memory,{},{}", msg.sent_from, msg.cause_by.as_str());
            let importance: i32 = match msg.cause_by {
                super::message::CauseBy::WritePrd | super::message::CauseBy::WriteDesign => 5,
                super::message::CauseBy::WriteCode | super::message::CauseBy::WriteCodeReview => 4,
                super::message::CauseBy::WriteTest => 3,
                _ => 3,
            };
            // Skip if already exists
            let exists: bool = conn.prepare("SELECT COUNT(*) FROM memories WHERE id = ?1")
                .ok()
                .and_then(|mut s| s.query_row(rusqlite::params![id], |r| r.get::<_, i64>(0)).ok())
                .map(|c| c > 0)
                .unwrap_or(false);
            if exists { continue; }
            let _ = memory_repo::insert_memory(
                conn, id, workspace, "metagpt_session",
                &summary, &tags, msg.cause_by.as_str(), importance, &now,
            );
        }
        tracing::info!(target: "metagpt::memory", "Persisted {} messages to workspace {}", self.storage.len(), workspace);
        Ok(())
    }

    /// Restore memory messages from SQLite for a workspace.
    pub fn restore_from(&mut self, conn: &rusqlite::Connection, workspace: &str) -> anyhow::Result<usize> {
        use crate::db::memory_repo;
        let rows = memory_repo::list_recent_memories(conn, workspace, 200)?;
        let count = rows.len();
        for row in rows {
            // Skip if already loaded (dedup by id)
            if self.storage.iter().any(|m| m.id == row.id) { continue; }
            let cause_by = match row.memory_type.as_str() {
                "prd" => super::message::CauseBy::WritePrd,
                "design" => super::message::CauseBy::WriteDesign,
                "code" => super::message::CauseBy::WriteCode,
                "review" => super::message::CauseBy::WriteCodeReview,
                "test" => super::message::CauseBy::WriteTest,
                _ => super::message::CauseBy::General,
            };
            let msg = super::message::Message {
                id: row.id,
                content: row.summary,
                role: "memory".to_string(),
                cause_by,
                send_to: std::collections::HashSet::new(),
                sent_from: row.conversation_id,
            };
            self.add(msg);
        }
        tracing::info!(target: "metagpt::memory", "Restored {} messages from workspace {}", count, workspace);
        Ok(count)
    }
}

impl Default for Memory { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::metagpt::message::{CauseBy, Message};

    #[test]
    fn test_add_and_count() {
        let mut mem = Memory::new();
        mem.add(Message::new("a", "u", CauseBy::WritePrd, "u"));
        mem.add(Message::new("b", "u", CauseBy::WriteDesign, "u"));
        assert_eq!(mem.count(), 2);
    }

    #[test]
    fn test_dedup() {
        let mut mem = Memory::new();
        let msg = Message::new("a", "u", CauseBy::WritePrd, "u");
        let id = msg.id.clone();
        mem.add(msg.clone());
        mem.add(msg); // same id, should be deduped
        assert_eq!(mem.count(), 1);
    }

    #[test]
    fn test_get_by_cause() {
        let mut mem = Memory::new();
        mem.add(Message::new("prd", "u", CauseBy::WritePrd, "u"));
        mem.add(Message::new("design", "u", CauseBy::WriteDesign, "u"));
        mem.add(Message::new("prd2", "u", CauseBy::WritePrd, "u"));
        let prd_msgs = mem.get_by_cause(&CauseBy::WritePrd);
        assert_eq!(prd_msgs.len(), 2);
    }

    #[test]
    fn test_get_relevant_truncation() {
        let mut mem = Memory::new();
        for i in 0..100 {
            mem.add(Message::new(format!("message {}", i), "u", CauseBy::General, "u"));
        }
        let ctx = mem.get_relevant(500);
        assert!(ctx.len() <= 500);
    }
}
