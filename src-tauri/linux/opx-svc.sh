#!/bin/sh
# opx 服务启动包装（安装引导设计 §6 P3）。
# 背景：opx 单进程双入口（形态 A），headless 仍走 tauri/tao 事件循环，
# 无 DISPLAY/WAYLAND 时 GTK 初始化失败 → tao event_loop panic（exit 101）。
# 因此：有真实显示则直连（桌面 Linux），无显示经 Xvfb 虚拟显示启动。
if [ -n "$DISPLAY" ] || [ -n "$WAYLAND_DISPLAY" ]; then
    exec /usr/bin/opx --headless
elif command -v xvfb-run >/dev/null 2>&1; then
    exec xvfb-run -a /usr/bin/opx --headless
else
    echo "[opx] 无显示环境且未安装 xvfb，无法启动（请安装: apt install xvfb）" >&2
    exit 1
fi
