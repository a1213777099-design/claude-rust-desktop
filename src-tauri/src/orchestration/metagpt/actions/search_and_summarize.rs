use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct SearchAndSummarize;

#[async_trait::async_trait]
impl Action for SearchAndSummarize {
    fn name(&self) -> &str { "SearchAndSummarize" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a search specialist. Search for information and provide a summary.\n\n             Use tools (WebSearch, WebFetch) to find relevant information.\n\n             ## Search Query\n{}\n\n             Output a concise summary of search results with sources.", context);
        let system = "You are a search specialist. Find and summarize information efficiently.";
        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
