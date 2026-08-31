use super::message::Message;
use super::memory::Memory;
use super::serialization::{SerializedEnvironment, SerializedMessage};
use std::collections::{HashMap, HashSet};

pub struct Environment {
    pub history: Memory,
    pub msg_buffer: HashMap<String, Vec<Message>>,
    pub watched_types: HashMap<String, HashSet<String>>,
}

impl Environment {
    pub fn new() -> Self {
        Self { history: Memory::new(), msg_buffer: HashMap::new(), watched_types: HashMap::new() }
    }

    pub fn subscribe(&mut self, role_name: &str, types: Vec<&str>) {
        let entry = self.watched_types.entry(role_name.to_string()).or_default();
        for t in types { entry.insert(t.to_string()); }
    }

    pub fn publish_message(&mut self, message: Message) {
        for (role, watched) in &self.watched_types {
            if watched.contains(message.cause_by.as_str()) || message.send_to.contains(role) {
                self.msg_buffer.entry(role.clone()).or_default().push(message.clone());
            }
        }
        self.history.add(message);
    }

    pub fn get_messages(&mut self, role_name: &str) -> Vec<Message> {
        self.msg_buffer.remove(role_name).unwrap_or_default()
    }

    pub fn peek_messages(&self, role_name: &str) -> Vec<&Message> {
        self.msg_buffer.get(role_name).map(|v| v.iter().collect()).unwrap_or_default()
    }

    pub fn serialize(&self) -> SerializedEnvironment {
        SerializedEnvironment {
            history: self.history.get_all().iter().map(|m| SerializedMessage {
                id: m.id.clone(), content: m.content.clone(), role: m.role.clone(),
                cause_by: m.cause_by.as_str().to_string(),
                send_to: m.send_to.clone(), sent_from: m.sent_from.clone(),
            }).collect(),
            msg_buffer: self.msg_buffer.iter().map(|(k, v)| {
                (k.clone(), v.iter().map(|m| SerializedMessage {
                    id: m.id.clone(), content: m.content.clone(), role: m.role.clone(),
                    cause_by: m.cause_by.as_str().to_string(),
                    send_to: m.send_to.clone(), sent_from: m.sent_from.clone(),
                }).collect())
            }).collect(),
            watched_types: self.watched_types.clone(),
        }
    }
}

impl Default for Environment { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::metagpt::message::{CauseBy, Message};

    #[test]
    fn test_publish_and_get() {
        let mut env = Environment::new();
        env.subscribe("TestRole", vec!["WritePrd"]);
        let msg = Message::new("test content", "user", CauseBy::WritePrd, "user");
        env.publish_message(msg);
        let msgs = env.get_messages("TestRole");
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].content, "test content");
    }

    #[test]
    fn test_peek_does_not_consume() {
        let mut env = Environment::new();
        env.subscribe("R", vec!["WritePrd"]);
        env.publish_message(Message::new("hello", "u", CauseBy::WritePrd, "u"));
        let peeked = env.peek_messages("R");
        assert_eq!(peeked.len(), 1);
        let got = env.get_messages("R");
        assert_eq!(got.len(), 1);
    }

    #[test]
    fn test_subscription_routing() {
        let mut env = Environment::new();
        env.subscribe("A", vec!["WritePrd"]);
        env.subscribe("B", vec!["WriteDesign"]);
        env.publish_message(Message::new("prd", "u", CauseBy::WritePrd, "u"));
        assert_eq!(env.get_messages("A").len(), 1);
        assert_eq!(env.get_messages("B").len(), 0);
    }
}
