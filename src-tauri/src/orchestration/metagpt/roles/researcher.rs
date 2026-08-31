use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::{CollectLinks, ConductResearch};
use crate::orchestration::metagpt::message::CauseBy;

/// Researcher role - conducts deep research using web search.
/// Matches MetaGPT's Researcher.
pub fn create_researcher() -> Role {
    let mut role = Role::new("Researcher", "Research Specialist",
        "Conduct thorough research and produce comprehensive reports",
        "Use multiple sources, verify facts, cite sources");
    role.add_action(Box::new(CollectLinks));
    role.add_action(Box::new(ConductResearch));
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
