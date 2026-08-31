use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WriteTest;

#[async_trait::async_trait]
impl Action for WriteTest {
    fn name(&self) -> &str { "WriteTest" }
    fn cause_by(&self) -> CauseBy { CauseBy::WriteTest }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a Test Engineer. Based on the code implementation below, write comprehensive tests.\n\n             Use tools (Read, Glob, Write) to:\n             1. Read the source code to understand what to test\n             2. Write test files with proper test cases\n             3. Run tests with Bash to verify they pass\n\n             ## Code Implementation\n{}\n\n             Output a summary of tests written and their results.", context);

        let system = "You are a senior Test Engineer. Write thorough tests that cover edge cases.                       Use tools to read code and write test files.";

        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
