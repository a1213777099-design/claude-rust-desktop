/// Environment state serialization used by `Environment::serialize`.
///
/// (The old WorkflowCheckpoint save/restore scaffolding was removed —
/// it was never wired into any route.)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEnvironment {
    pub history: Vec<SerializedMessage>,
    pub msg_buffer: HashMap<String, Vec<SerializedMessage>>,
    pub watched_types: HashMap<String, HashSet<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedMessage {
    pub id: String,
    pub content: String,
    pub role: String,
    pub cause_by: String,
    pub send_to: HashSet<String>,
    pub sent_from: String,
}

