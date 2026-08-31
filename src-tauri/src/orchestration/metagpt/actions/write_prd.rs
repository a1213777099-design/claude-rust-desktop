use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WritePrd;

#[async_trait::async_trait]
impl Action for WritePrd {
    fn name(&self) -> &str { "WritePrd" }
    fn cause_by(&self) -> CauseBy { CauseBy::WritePrd }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a Product Manager. Before writing the PRD, you MUST use tools to understand the project:
- Use Glob to find project files
- Use Read to read key files (package.json, Cargo.toml, README, main source files)
- Use ListDir to explore directory structure

Then write a PRD based on your ACTUAL findings and the context below.\n\n             ## Context\n{}\n\n             Output a structured PRD with: background, goals, user_stories, requirements, acceptance_criteria, risks.\n             Use markdown format.", context);

        let system = "You are a senior Product Manager. You MUST use tools to read the actual codebase before writing. Never make up project details. All output in Chinese.";
        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
