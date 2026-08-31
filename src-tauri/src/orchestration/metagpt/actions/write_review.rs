use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::orchestration::metagpt::review_verdict::ReviewVerdict;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct WriteReview;

#[async_trait::async_trait]
impl Action for WriteReview {
    fn name(&self) -> &str { "WriteReview" }
    fn cause_by(&self) -> CauseBy { CauseBy::WriteCodeReview }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are a senior Code Reviewer. Review the code implementation below.\n\n             Use tools (Read, Glob, Grep) to inspect the actual code files if needed.\n\n             ## Code Implementation\n{}\n\n             Your review MUST include at the end:\n             - quality_score: X (1-10)\n             - approved: true/false (true if score >= 7)\n             - issues: list of critical problems\n             - suggestions: list of improvements\n\n             If the code has critical issues, set approved: false and explain what must be fixed.", context);

        let system = "You are a senior Code Reviewer. Be thorough but fair.                       Use tools to verify code actually exists and compiles.                       Always end with quality_score and approved fields.";

        let workspace = std::env::var("METAGPT_WORKSPACE").unwrap_or_else(|_| ".".to_string());
        let output = crate::orchestration::metagpt::tool_loop::run_with_tools_named(&prompt, system, provider, &workspace, self.name()).await?;

        // Parse verdict for logging
        let verdict = ReviewVerdict::from_text(&output);
        tracing::info!(target: "metagpt::review",
            "Review verdict: approved={}, score={}", verdict.approved, verdict.quality_score);

        Ok(output)
    }
}
