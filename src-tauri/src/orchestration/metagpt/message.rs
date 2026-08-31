use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum CauseBy {
    UserRequirement,
    WritePrd,
    WriteDesign,
    WriteCode,
    WriteCodeReview,
    WriteTest,
    RunCode,
    DebugError,
    General,
}

impl CauseBy {
    /// 从动作名字符串还原 CauseBy（工作流续跑回放用）
    pub fn from_name(s: &str) -> Self {
        match s {
            "UserRequirement" => Self::UserRequirement,
            "WritePrd" => Self::WritePrd,
            "WriteDesign" => Self::WriteDesign,
            "WriteCode" => Self::WriteCode,
            "WriteCodeReview" => Self::WriteCodeReview,
            "WriteTest" => Self::WriteTest,
            "RunCode" => Self::RunCode,
            "DebugError" => Self::DebugError,
            _ => Self::General,
        }
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::UserRequirement => "UserRequirement",
            Self::WritePrd => "WritePrd",
            Self::WriteDesign => "WriteDesign",
            Self::WriteCode => "WriteCode",
            Self::WriteCodeReview => "WriteCodeReview",
            Self::WriteTest => "WriteTest",
            Self::RunCode => "RunCode",
            Self::DebugError => "DebugError",
            Self::General => "General",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub role: String,
    pub cause_by: CauseBy,
    pub send_to: HashSet<String>,
    pub sent_from: String,
}

impl Message {
    pub fn new(content: impl Into<String>, role: impl Into<String>, cause_by: CauseBy, sent_from: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            content: content.into(),
            role: role.into(),
            cause_by,
            send_to: HashSet::new(),
            sent_from: sent_from.into(),
        }
    }

    pub fn send_to(mut self, r: impl Into<String>) -> Self {
        self.send_to.insert(r.into());
        self
    }

    pub fn with_metadata(mut self, _key: &str, _value: serde_json::Value) -> Self {
        self
    }
}
