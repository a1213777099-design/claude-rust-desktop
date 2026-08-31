use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::WriteTest;

pub fn create_qa_engineer() -> Role {
    let mut role = Role::new("QaEngineer", "QA Engineer",
        "通过全面测试确保软件质量",
        "编写覆盖边界情况、集成和回归的测试");
    role.add_action(Box::new(WriteTest));
    role.watch(vec![
        crate::orchestration::metagpt::message::CauseBy::WriteCode,
        crate::orchestration::metagpt::message::CauseBy::WriteCodeReview,
        // 返工修复完成（DebugError）后，QA 在同一轮内基于修复结果出测试
        crate::orchestration::metagpt::message::CauseBy::DebugError,
    ]);
    role
}
