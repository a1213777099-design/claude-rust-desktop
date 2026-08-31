use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::RunCode;
use crate::orchestration::metagpt::message::CauseBy;

pub fn create_devops() -> Role {
    let mut role = Role::new("DevOps", "DevOps Engineer",
        "验证构建和部署就绪状态",
        "运行构建、测试和部署验证");
    role.add_action(Box::new(RunCode));
    role.watch(vec![
        CauseBy::WriteCode,
        CauseBy::WriteTest,
    ]);
    role
}
