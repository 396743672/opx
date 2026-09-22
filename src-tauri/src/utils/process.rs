//! 子进程静默启动辅助。
//!
//! Windows 上，GUI（windows 子系统）进程启动**控制台子程序**时，若未指定
//! `CREATE_NO_WINDOW`，系统会为该子进程新建一个控制台窗口，表现为「黑窗一闪而过」。
//! 本模块统一封装该标志，供 netstat / taskkill / jcmd / java / node 等控制台子程序复用。
//!
//! 语义来源（现场核实）：Windows Process Creation Flags —
//! `CREATE_NO_WINDOW` = `0x08000000`，「控制台程序以无控制台窗口方式运行，
//! 因此不为其设置控制台句柄」。
//! <https://learn.microsoft.com/en-us/windows/win32/procthread/process-creation-flags>

use std::ffi::OsStr;
use std::process::Command;

/// Windows `CREATE_NO_WINDOW`：控制台子程序不创建控制台窗口。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 让 `cmd` 启动的子进程不弹出控制台窗口（Windows）；非 Windows 平台为空操作。
///
/// 必须在 `spawn` / `output` / `status` 之前调用（标志在 `CreateProcess` 时生效）。
pub fn hide_console(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    let _ = cmd;
}

/// 构造一个「静默启动」的命令。
///
/// 等价于 `let mut c = Command::new(program); hide_console(&mut c); c`。
pub fn hidden(program: impl AsRef<OsStr>) -> Command {
    let mut cmd = Command::new(program);
    hide_console(&mut cmd);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归：静默标志不得影响子进程正常执行与 stdout 捕获。
    ///
    /// 注：「是否真的不弹窗」无法单测——cargo test 自身是控制台进程，子进程会复用其控制台；
    /// 弹窗只在 GUI（windows 子系统）父进程下出现，故该行为需在应用内实机确认。
    #[test]
    fn hidden_command_still_runs_and_captures_output() {
        #[cfg(windows)]
        let out = hidden("cmd").args(["/C", "echo", "ok"]).output();
        #[cfg(not(windows))]
        let out = hidden("echo").arg("ok").output();

        let out = out.expect("spawn 应成功");
        assert!(out.status.success(), "退出码应为 0");
        assert!(
            String::from_utf8_lossy(&out.stdout).contains("ok"),
            "应捕获到子进程输出"
        );
    }

    /// `hide_console` 对已构建的命令同样生效（调用点两种写法都覆盖）。
    #[test]
    fn hide_console_is_chainable_on_existing_command() {
        let mut cmd = Command::new(if cfg!(windows) { "cmd" } else { "echo" });
        hide_console(&mut cmd);
        assert_eq!(cmd.get_program(), if cfg!(windows) { "cmd" } else { "echo" });
    }
}
