use crate::orchestration::metagpt::role::Role;
use crate::orchestration::metagpt::actions::WritePrd;

pub fn create_product_manager() -> Role {
    let mut role = Role::new("ProductManager", "Product Manager", 
        "分析需求并创建全面的PRD",
        "请全面考虑，包括边界情况，使用工具扫描项目");
    role.add_action(Box::new(WritePrd));
    role.watch(vec![
        crate::orchestration::metagpt::message::CauseBy::UserRequirement,
    ]);
    role
}
