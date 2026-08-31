use super::message::{CauseBy, Message};
use anyhow::Result;

#[async_trait::async_trait]
pub trait Action: Send + Sync {
    fn name(&self) -> &str;
    fn cause_by(&self) -> CauseBy;
    async fn run(&self, context: &str, provider: &crate::native_engine::provider_manager::ResolvedProvider) -> Result<String>;
    fn to_message(&self, output: &str, sender: &str) -> Message {
        Message::new(output, "assistant", self.cause_by(), sender)
    }
}
