use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::SearchAndSummarize;
use crate::orchestration::metagpt::message::CauseBy;

/// Searcher role - searches and summarizes information.
/// Matches MetaGPT's Searcher.
pub fn create_searcher() -> Role {
    let mut role = Role::new("Searcher", "Search Specialist",
        "Search for information and provide concise summaries",
        "Be efficient, find relevant sources, summarize key points");
    role.add_action(Box::new(SearchAndSummarize));
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
