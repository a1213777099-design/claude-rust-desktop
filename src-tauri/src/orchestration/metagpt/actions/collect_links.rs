use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct CollectLinks;

#[async_trait::async_trait]
impl Action for CollectLinks {
    fn name(&self) -> &str { "CollectLinks" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a research assistant. Collect relevant URLs and links for the following topic.\n\n             ## Topic\n{}\n\n             Output a list of relevant URLs with brief descriptions. Use markdown format.", context);
        let system = "You are a research assistant. Find and list relevant URLs for research topics.";
        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
