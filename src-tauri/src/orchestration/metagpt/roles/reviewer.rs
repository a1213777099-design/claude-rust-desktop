use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::WriteReview;

pub fn create_reviewer() -> Role {
    let mut role = Role::new("Reviewer", "Code Reviewer",
        "审查代码质量并提供反馈",
        "请严格但公正。使用工具检查实际代码文件。");
    role.add_action(Box::new(WriteReview));
    role.watch(vec![
        crate::orchestration::metagpt::message::CauseBy::WriteCode,
    ]);
    role
}
