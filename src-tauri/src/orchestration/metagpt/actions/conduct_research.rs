use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct ConductResearch;

#[async_trait::async_trait]
impl Action for ConductResearch {
    fn name(&self) -> &str { "ConductResearch" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a senior researcher. Conduct thorough research on the following topic.\n\n             Use tools (WebFetch, WebSearch, Read) to gather information.\n\n             ## Research Topic\n{}\n\n             Output a comprehensive research report with: summary, key_findings, sources, conclusions.\n             Use markdown format.", context);
        let system = "You are a senior researcher. Conduct thorough, well-sourced research.";
        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
