use crate::models::software::SoftwareStatus;
use crate::services::software_manager::SoftwareManager;

/// 验证前置依赖，返回所有未运行的依赖名称列表

/// 获取可选的前置依赖候选列表（本机已安装的 MySQL/Redis/Nginx/MinIO）
pub fn list_dependency_candidates(
    software_mgr: &SoftwareManager,
) -> Vec<crate::models::software::InstalledSoftware> {
    let installed = software_mgr.get_installed();
    let managed_keys = ["mysql", "redis", "nginx", "minio"];
    installed
        .into_iter()
        .filter(|s| managed_keys.contains(&s.key.as_str()))
        .collect()
}
