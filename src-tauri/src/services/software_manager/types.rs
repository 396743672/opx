use crate::models::software::{InstalledSoftware, SoftwareMeta, InstallParams};
use anyhow::Result;
use std::path::Path;

pub trait SoftwareProvider {
    /// 获取软件元信息
    fn meta(&self) -> SoftwareMeta;

    /// 获取下载 URL
    fn download_url(&self, version: &str) -> String;

    /// 安装后初始化配置
    fn post_install(&self, install_path: &Path, params: &InstallParams) -> Result<()>;

    /// 获取启动命令
    fn start_command(&self, software: &InstalledSoftware) -> Result<std::process::Command>;

    /// 获取停止命令
    fn stop_command(&self, software: &InstalledSoftware) -> Result<()>;
}