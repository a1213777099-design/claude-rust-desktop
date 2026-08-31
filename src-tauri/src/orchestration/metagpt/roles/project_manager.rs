use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::message::CauseBy;

pub struct WriteProjectPlan;

#[async_trait::async_trait]
impl crate::orchestration::metagpt::action::Action for WriteProjectPlan {
    fn name(&self) -> &str { "WriteProjectPlan" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &crate::native_engine::provider_manager::ResolvedProvider) -> anyhow::Result<String> {
        let prompt = format!(
            "你是一位项目经理。根据以下PRD和设计文档，创建项目计划。\n\n             ## 上下文\n{}\n\n             请输出：里程碑、时间线、风险评估、资源分配、任务分解。\n             使用Markdown格式。所有输出使用中文。", context);

        let system = "你是一位资深项目经理。请创建实际可行的项目计划。所有输出使用中文。";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}

pub fn create_project_manager() -> Role {
    let mut role = Role::new("ProjectManager", "Project Manager",
        "规划和协调项目执行",
        "创建实际可行的时间线并管理依赖关系");
    role.add_action(Box::new(WriteProjectPlan));
    role.watch(vec![
        CauseBy::WritePrd,
        CauseBy::WriteDesign,
    ]);
    role
}
