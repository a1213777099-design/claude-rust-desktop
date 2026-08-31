use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WriteCode;

#[async_trait::async_trait]
impl Action for WriteCode {
    fn name(&self) -> &str { "WriteCode" }
    fn cause_by(&self) -> CauseBy { CauseBy::WriteCode }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "你是一位软件工程师。根据以下设计文档，实现代码。\n\n             你必须使用提供的工具：\n             - 使用Glob查找现有文件\n             - 使用Read理解现有代码\n             - 使用Write创建新文件\n             - 使用Edit修改现有文件\n             - 使用Bash运行测试或验证编译\n\n             不要编造代码。请先阅读代码库，然后实现。\n\n             ## 设计文档\n{}\n\n             实现完成后，请输出你所做的工作总结。", context);

        let system = "你是一位资深软件工程师。你必须使用工具来读写文件。不要在没有阅读代码库的情况下编造代码。所有输出使用中文。";

        // Use tool loop - agent will actually call tools
        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await
    }
}
