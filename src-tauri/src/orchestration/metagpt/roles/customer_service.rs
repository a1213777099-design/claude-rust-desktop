use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::message::CauseBy;

/// Customer service role.
/// Matches MetaGPT's CustomerService.
pub fn create_customer_service() -> Role {
    let mut role = Role::new("CustomerService", "Customer Service Representative",
        "Provide helpful customer support and resolve issues",
        "Be polite, empathetic, and follow company policies. Soothe customer emotions first.");
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
