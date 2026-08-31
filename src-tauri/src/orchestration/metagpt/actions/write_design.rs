use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WriteDesign;

#[async_trait::async_trait]
impl Action for WriteDesign {
    fn name(&self) -> &str { "WriteDesign" }
    fn cause_by(&self) -> CauseBy { CauseBy::WriteDesign }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a Solution Architect. Before writing the design, you MUST use tools to explore the codebase:
- Use Glob to find existing source files
- Use Read to read key source files and understand current architecture
- Use ListDir to see directory structure
- Use Grep to find relevant patterns

Then write a technical design based on ACTUAL code and the PRD below.\n\n             ## PRD\n{}\n\n             Output: architecture_overview, module_design, data_model, api_design, tech_stack, file_structure.\n             Use markdown format.", context);

        let system = "You are a senior Solution Architect. You MUST use tools to read the actual codebase before designing. Never invent architecture that does not exist. All output in Chinese.";
        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
