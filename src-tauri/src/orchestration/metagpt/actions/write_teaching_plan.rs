use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WriteTeachingPlan;

#[async_trait::async_trait]
impl Action for WriteTeachingPlan {
    fn name(&self) -> &str { "WriteTeachingPlan" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are an experienced teacher. Create a comprehensive teaching plan.\n\n             ## Topic\n{}\n\n             Output: learning_objectives, lesson_outline, activities, assessment, resources.\n             Use markdown format.", context);
        let system = "You are an experienced teacher. Create clear, structured teaching plans.";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}
