use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WriteDirectory;
pub struct WriteContent;

#[async_trait::async_trait]
impl Action for WriteDirectory {
    fn name(&self) -> &str { "WriteDirectory" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a tutorial author. Create a tutorial directory/outline.\n\n             ## Topic\n{}\n\n             Output a structured table of contents with chapters and sections.\n             Use markdown format.", context);
        let system = "You are a tutorial author. Create well-organized tutorial outlines.";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}

#[async_trait::async_trait]
impl Action for WriteContent {
    fn name(&self) -> &str { "WriteContent" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a tutorial author. Write detailed tutorial content based on the outline.\n\n             ## Outline\n{}\n\n             Write detailed, beginner-friendly content with code examples.\n             Use markdown format.", context);
        let system = "You are a tutorial author. Write clear, detailed tutorial content.";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}
