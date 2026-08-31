/// Smart context window management.
///
/// Matches MetaGPT's context management with summarization,
/// sliding window, and priority-based message selection.

use super::message::Message;

const DEFAULT_MAX_CHARS: usize = 12000;
const SUMMARY_THRESHOLD: usize = 8000;

pub struct ContextManager {
    pub max_chars: usize,
    pub strategy: ContextStrategy,
}

#[derive(Debug, Clone)]
pub enum ContextStrategy {
    /// Keep most recent messages (sliding window)
    SlidingWindow,
    /// Prioritize by message type (reviews > code > design > prd)
    PriorityBased,
    /// Summarize old messages when context is too large
    Summarize,
}

impl ContextManager {
    pub fn new(max_chars: usize) -> Self {
        Self { max_chars, strategy: ContextStrategy::PriorityBased }
    }

    pub fn with_strategy(mut self, strategy: ContextStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Build context string from messages, respecting max_chars.
    pub fn build_context(&self, messages: &[&Message]) -> String {
        match self.strategy {
            ContextStrategy::SlidingWindow => self.sliding_window(messages),
            ContextStrategy::PriorityBased => self.priority_based(messages),
            ContextStrategy::Summarize => self.with_summarization(messages),
        }
    }

    fn sliding_window(&self, messages: &[&Message]) -> String {
        let mut ctx = String::new();
        for msg in messages.iter().rev() {
            let line = format!("[{}] {}: {}
", msg.cause_by.as_str(), msg.sent_from, msg.content);
            if ctx.len() + line.len() > self.max_chars { break; }
            ctx.insert_str(0, &line);
        }
        ctx
    }

    fn priority_based(&self, messages: &[&Message]) -> String {
        // Priority: WriteCodeReview > WriteCode > DebugError > WriteDesign > WritePrd > others
        let priority = |cb: &super::message::CauseBy| -> u8 {
            match cb {
                super::message::CauseBy::WriteCodeReview => 6,
                super::message::CauseBy::DebugError => 5,
                super::message::CauseBy::WriteCode => 4,
                super::message::CauseBy::WriteDesign => 3,
                super::message::CauseBy::WritePrd => 2,
                super::message::CauseBy::WriteTest => 1,
                _ => 0,
            }
        };

        let mut sorted: Vec<&&Message> = messages.iter().collect();
        sorted.sort_by(|a, b| {
            let pa = priority(&a.cause_by);
            let pb = priority(&b.cause_by);
            pb.cmp(&pa).then_with(|| b.id.cmp(&a.id))
        });

        let mut ctx = String::new();
        for msg in sorted {
            let line = format!("[{}] {}: {}
", msg.cause_by.as_str(), msg.sent_from, msg.content);
            if ctx.len() + line.len() > self.max_chars { break; }
            ctx.push_str(&line);
        }
        ctx
    }

    fn with_summarization(&self, messages: &[&Message]) -> String {
        let total: usize = messages.iter().map(|m| m.content.len()).sum();
        if total <= self.max_chars {
            return self.sliding_window(messages);
        }

        // Summarize older messages, keep recent ones full
        let mut ctx = String::new();
        let recent_count = (messages.len() / 3).max(3);
        let (older, recent) = messages.split_at(messages.len().saturating_sub(recent_count));

        // Summarize older messages
        if !older.is_empty() {
            ctx.push_str(&format!("=== Previous context ({} messages summarized) ===
", older.len()));
            for msg in older {
                let summary = if msg.content.len() > 200 {
                    format!("{}...", &msg.content[..200])
                } else {
                    msg.content.clone()
                };
                ctx.push_str(&format!("[{}] {}: {}
", msg.cause_by.as_str(), msg.sent_from, summary));
            }
            ctx.push('\n');
        }

        // Keep recent messages full
        for msg in recent {
            let line = format!("[{}] {}: {}
", msg.cause_by.as_str(), msg.sent_from, msg.content);
            if ctx.len() + line.len() > self.max_chars { break; }
            ctx.push_str(&line);
        }
        ctx
    }
}

impl Default for ContextManager {
    fn default() -> Self { Self::new(DEFAULT_MAX_CHARS) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::metagpt::message::{CauseBy, Message};

    fn make_msgs(n: usize) -> Vec<Message> {
        (0..n).map(|i| Message::new(format!("msg {}", i), "u", CauseBy::General, "u")).collect()
    }

    #[test]
    fn test_sliding_window_respects_limit() {
        let cm = ContextManager::new(200).with_strategy(ContextStrategy::SlidingWindow);
        let msgs = make_msgs(100);
        let refs: Vec<&Message> = msgs.iter().collect();
        let ctx = cm.build_context(&refs);
        assert!(ctx.len() <= 200);
    }

    #[test]
    fn test_priority_based_gives_higher_weight() {
        let cm = ContextManager::new(5000).with_strategy(ContextStrategy::PriorityBased);
        let mut msgs = make_msgs(5);
        msgs[0] = Message::new("design doc", "u", CauseBy::WriteDesign, "u");
        msgs[1] = Message::new("review feedback", "u", CauseBy::WriteCodeReview, "u");
        let refs: Vec<&Message> = msgs.iter().collect();
        let ctx = cm.build_context(&refs);
        // Review should appear before design
        assert!(ctx.find("review feedback").unwrap_or(999) < ctx.find("design doc").unwrap_or(999));
    }
}
