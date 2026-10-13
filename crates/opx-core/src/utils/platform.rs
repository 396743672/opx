/// 返回当前运行平台标识，供 catalog 版本发现按 OS 选择下载 URL。
///
/// 用于替代编译期 `#[cfg(windows)]` 把整张 catalog 锁死为 Windows-only（P2-1）：
/// 非 Windows 构建下 catalog 不再为空，版本/镜像 URL 按运行时 OS 选对应官方包。
/// 注意这只是「选 URL」的维度——真正的跨平台还需要每个 provider 提供对应平台的
/// 下载地址、解压后目录结构与启动命令（逐软件补，详见审计文档第 6 章 #2/#3）。
pub fn current_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "linux" => "linux",
        "macos" => "macos",
        other => other,
    }
}
