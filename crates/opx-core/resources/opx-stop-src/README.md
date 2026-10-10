# opx-stop.jar —— Windows 优雅停止 agent

停止 Spring Boot 应用时，opx 通过这个 jar 让目标 JVM **自己**退出，从而走完整的
JVM 关闭流程（shutdown hook / Spring 优雅停机 / `@PreDestroy` / 连接池关闭）。

## 为什么需要它

Windows 上没有 SIGTERM，而应用零配置时可用的替代通道全部实测无效：

| 手段 | 结果 |
|---|---|
| `jcmd <pid> Shutdown` | 命令不存在（JDK 8 / 21 均报 `Unknown diagnostic command`） |
| `taskkill /PID`（不带 `/F`） | 发 WM_CLOSE，只对进程自拥窗口有效，控制台程序被直接拒绝 |
| `AttachConsole` + `CTRL_BREAK` | API 返回成功但**信号不送达**，shutdown hook 不执行 |

attach + agent 注入是唯一可行且**应用无需任何配置**的通道。

## 内容

- `AttachMain.java` —— 启动器，在主进程外运行，用 JDK 自带 attach API 注入 agent
- `StopAgent.java` —— 被注入目标 JVM，稍后调用 `System.exit(0)`

两者的 class 文件 + `META-INF/MANIFEST.MF`（`Main-Class: AttachMain`、
`Agent-Class: StopAgent`）打成一个 jar，因此 opx 只需携带一个文件：
既用 `java -cp opx-stop.jar AttachMain <pid> <jar>` 启动，也把同一个路径交给
`loadAgent` 当 agent jar。

## 重新构建

jar 是二进制产物，源码在此以保证可复现。用 **JDK 8** 编译（`com.sun.tools.attach`
在 tools.jar 里，`javac --release 8` 看不到它，必须走 JDK 8 的 javac + classpath）：

```bash
J8=<JDK8_HOME>/bin
OUT=$(mktemp -d)
"$J8/javac" -encoding UTF-8 -cp "<JDK8_HOME>/lib/tools.jar" -d "$OUT" AttachMain.java StopAgent.java
printf 'Manifest-Version: 1.0\r\nMain-Class: AttachMain\r\nAgent-Class: StopAgent\r\nCan-Retransform-Classes: false\r\n\r\n' > "$OUT/manifest.txt"
"$J8/jar" cfm ../opx-stop.jar "$OUT/manifest.txt" -C "$OUT" .
```

产出的 class 版本为 52，JDK 8 ~ 25 均可加载。

## 运行时约束（均已实测）

- 启动器**需要 JDK**（attach API 只在 JDK 里）；目标可以是 JRE
- 启动器与目标**不必同主版本**：实测 JDK 8 / 21 / 25 注入 JRE 8 / 17 / 21 全部
  9/9 生效（目标 shutdown hook 执行、进程自行退出）。跨版本时**客户端**会解析
  响应失败并打印 `Non-numeric value found` / `AgentLoadException`，但命令已送达
  并被执行——**不能按启动器退出码判定成败**。同主版本只是让客户端能干净地拿到
  响应，故仅作「优先」而非必要条件
- 目标 JVM 若带 `-XX:+DisableAttachMechanism` 则无法注入
- JDK 21+ 注入时目标 JVM 的 stderr 会打 4 行 JEP 451 警告，属预期
