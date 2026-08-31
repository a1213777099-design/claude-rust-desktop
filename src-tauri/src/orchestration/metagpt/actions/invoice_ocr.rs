use crate::orchestration::metagpt::message::CauseBy;
use crate::orchestration::metagpt::action::Action;
use crate::native_engine::provider_manager::ResolvedProvider;
use anyhow::Result;

pub struct InvoiceOCR;
pub struct GenerateTable;
pub struct ReplyQuestion;

#[async_trait::async_trait]
impl Action for InvoiceOCR {
    fn name(&self) -> &str { "InvoiceOCR" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "You are an OCR specialist. Extract text and data from invoice images.\n\n             ## Invoice Data\n{}\n\n             Output extracted fields: invoice_number, date, items, amounts, total.", context);
        let system = "You are an OCR specialist. Extract invoice data accurately.";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}

#[async_trait::async_trait]
impl Action for GenerateTable {
    fn name(&self) -> &str { "GenerateTable" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "Generate a structured table from invoice data.\n\n             ## Invoice Data\n{}\n\n             Output a markdown table with columns: Item, Quantity, Unit Price, Total.", context);
        let system = "Generate clean, formatted tables from invoice data.";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}

#[async_trait::async_trait]
impl Action for ReplyQuestion {
    fn name(&self) -> &str { "ReplyQuestion" }
    fn cause_by(&self) -> CauseBy { CauseBy::General }

    async fn run(&self, context: &str, provider: &ResolvedProvider) -> Result<String> {
        let prompt = format!(
            "Answer questions about invoice data.\n\n             ## Context\n{}\n\n             Provide clear, accurate answers about the invoice.", context);
        let system = "Answer invoice-related questions accurately.";
        crate::orchestration::metagpt::tool_loop::run_simple(&prompt, system, provider).await
    }
}
