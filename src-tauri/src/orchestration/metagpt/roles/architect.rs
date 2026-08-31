use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::WriteDesign;

pub fn create_architect() -> Role {
    let mut role = Role::new("Architect", "Solution Architect",
        "根据PRD设计技术架构",
        "请考虑可扩展性、可维护性和现有代码库");
    role.add_action(Box::new(WriteDesign));
    role.watch(vec![
        crate::orchestration::metagpt::message::CauseBy::WritePrd,
    ]);
    role
}
