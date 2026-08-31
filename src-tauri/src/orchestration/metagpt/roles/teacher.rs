use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::WriteTeachingPlan;
use crate::orchestration::metagpt::message::CauseBy;

/// Teacher role - creates teaching plans.
/// Matches MetaGPT's Teacher.
pub fn create_teacher() -> Role {
    let mut role = Role::new("Teacher", "Teacher",
        "Create comprehensive teaching plans and educational content",
        "Be clear, patient, and adapt to student level");
    role.add_action(Box::new(WriteTeachingPlan));
    role.watch(vec![CauseBy::UserRequirement, CauseBy::General]);
    role
}
