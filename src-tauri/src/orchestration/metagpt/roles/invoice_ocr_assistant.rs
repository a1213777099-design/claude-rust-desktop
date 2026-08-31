use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::invoice_ocr::{InvoiceOCR, GenerateTable, ReplyQuestion};
use crate::orchestration::metagpt::message::CauseBy;

/// Invoice OCR assistant role - processes invoices.
/// Matches MetaGPT's InvoiceOcrAssistant.
pub fn create_invoice_ocr_assistant() -> Role {
    let mut role = Role::new("InvoiceOcrAssistant", "Invoice OCR Specialist",
        "Process invoices using OCR and extract structured data",
        "Be accurate, extract all relevant fields, handle various invoice formats");
    role.add_action(Box::new(InvoiceOCR));
    role.add_action(Box::new(GenerateTable));
    role.add_action(Box::new(ReplyQuestion));
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
