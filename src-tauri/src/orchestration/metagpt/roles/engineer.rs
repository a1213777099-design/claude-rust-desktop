use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::{WriteCode, DebugError};

pub fn create_engineer() -> Role {
    let mut role = Role::new("Engineer", "Software Developer",
        "根据架构设计实现代码",
        "编写清晰、生产级代码。先使用工具阅读现有代码。");
    role.add_action(Box::new(WriteCode));
    role.watch(vec![
        crate::orchestration::metagpt::message::CauseBy::WriteDesign,
        crate::orchestration::metagpt::message::CauseBy::WriteCodeReview,
        crate::orchestration::metagpt::message::CauseBy::DebugError,
    ]);
    role
}

pub fn create_engineer_for_rework() -> Role {
    // 名字必须与正式 Engineer 区分：completed_roles 按角色名去重，
    // 且前端按该名回映射到工程师卡片
    let mut role = Role::new("EngineerRework", "Software Developer (Rework)",
        "修复代码审查员指出的问题",
        "处理所有审查反馈。使用工具读取和修复代码。");
    role.add_action(Box::new(DebugError));
    role.watch(vec![
        crate::orchestration::metagpt::message::CauseBy::WriteCodeReview,
    ]);
    role
}
