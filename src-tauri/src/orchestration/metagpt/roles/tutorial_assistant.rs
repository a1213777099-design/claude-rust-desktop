use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::write_tutorial::{WriteDirectory, WriteContent};
use crate::orchestration::metagpt::message::CauseBy;

/// Tutorial assistant role - generates tutorial documents.
/// Matches MetaGPT's TutorialAssistant.
pub fn create_tutorial_assistant() -> Role {
    let mut role = Role::new("TutorialAssistant", "Tutorial Author",
        "Generate comprehensive tutorial documents in markdown format",
        "Write beginner-friendly content with clear examples");
    role.add_action(Box::new(WriteDirectory));
    role.add_action(Box::new(WriteContent));
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
