use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct DebugError;

#[async_trait::async_trait]
impl Action for DebugError {
    fn name(&self) -> &str { "DebugError" }
    fn cause_by(&self) -> CauseBy { CauseBy::DebugError }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "你是一位调试专家。请修复代码中的错误。\n\n             使用工具（Read、Edit、Bash、Grep）来：\n             1. 读取错误信息和堆栈跟踪\n             2. 找到问题代码\n             3. 修复问题\n             4. 验证修复是否编译通过/测试通过\n\n             ## 错误上下文\n{}\n\n             请输出：根本原因、应用的修复、验证结果、总结。所有输出使用中文。", context);

        let system = "你是一位调试专家。请系统性地诊断和修复错误。使用工具读取代码、应用修复、并验证。所有输出使用中文。";

        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
