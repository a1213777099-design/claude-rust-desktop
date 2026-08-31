use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct RunCode;

#[async_trait::async_trait]
impl Action for RunCode {
    fn name(&self) -> &str { "RunCode" }
    fn cause_by(&self) -> CauseBy { CauseBy::RunCode }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a DevOps Engineer. Verify the code implementation works correctly.\n\n             Use tools (Bash, Read) to:\n             1. Check if the project compiles/builds\n             2. Run existing tests\n             3. Verify key functionality works\n\n             ## Implementation Context\n{}\n\n             Output: build_status, test_results, issues_found, summary.", context);

        let system = "你是一位DevOps工程师。请运行构建和测试命令来验证代码质量。准确报告结果。所有输出使用中文。";

        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
