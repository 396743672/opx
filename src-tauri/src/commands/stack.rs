//! B 扩展（一键启动栈 Stack）Tauri 命令层
//!
//! 栈 CRUD / 编排 / 导出导入命令，全部复用 `State<'_, Arc<StackManager>>`，
//! 与现有 `start_software` / `start_springboot_app` 风格一致，返回 `Result<T, String>`。
//!
//! 本文件按实现计划分批落地：
//! - T2：list_stacks / get_stack / create_stack / update_stack / delete_stack
//! - T3：start_stack / stop_stack / restart_stack
//! - T4：export_stack / import_stack

use std::sync::Arc;

use tauri::State;

use crate::models::stack::{CreateStackPayload, Stack, UpdateStackPayload};
use crate::services::stack_manager::StackManager;

/// 列出所有栈
#[tauri::command]
pub async fn list_stacks(
    manager: State<'_, Arc<StackManager>>,
) -> Result<Vec<Stack>, String> {
    Ok(manager.list())
}

/// 获取单个栈
#[tauri::command]
pub async fn get_stack(
    manager: State<'_, Arc<StackManager>>,
    id: String,
) -> Result<Stack, String> {
    manager
        .get(&id)
        .ok_or_else(|| format!("未找到栈: {}", id))
}

/// 创建栈（保存前做环检测，有环返回环路径错误，不写入）
#[tauri::command]
pub async fn create_stack(
    manager: State<'_, Arc<StackManager>>,
    payload: CreateStackPayload,
) -> Result<Stack, String> {
    manager.create(payload).map_err(|e| e.to_string())
}

/// 更新栈（保存前做环检测，有环返回环路径错误）
#[tauri::command]
pub async fn update_stack(
    manager: State<'_, Arc<StackManager>>,
    id: String,
    payload: UpdateStackPayload,
) -> Result<Stack, String> {
    manager.update(&id, payload).map_err(|e| e.to_string())
}

/// 删除栈
#[tauri::command]
pub async fn delete_stack(
    manager: State<'_, Arc<StackManager>>,
    id: String,
) -> Result<(), String> {
    manager.delete(&id).map(|_| ())
}
