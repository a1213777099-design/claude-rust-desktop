use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::message::CauseBy;

/// Generic assistant role - can handle any task.
/// Matches MetaGPT's Assistant which is a fork-style meta role.
pub fn create_assistant() -> Role {
    let mut role = Role::new("Assistant", "General Assistant",
        "Assist with any task using available tools",
        "Be helpful, thorough, and use tools to complete tasks");
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
