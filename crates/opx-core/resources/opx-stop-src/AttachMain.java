import com.sun.tools.attach.VirtualMachine;

/**
 * opx 优雅停止启动器。
 *
 * 用法：java -cp [<JAVA_HOME>/lib/tools.jar;]opx-stop.jar AttachMain <pid> <同一个 jar 的路径>
 *
 * 作用：通过 JDK 自带的 attach API 把 StopAgent 注入目标 JVM，让目标 JVM
 * **自己**调用 System.exit(0)——这一步会走完整的 JVM 关闭流程，shutdown hook、
 * Spring 的优雅停机、@PreDestroy、连接池关闭全部生效。
 *
 * 为什么需要它：Windows 上没有 SIGTERM，而 `jcmd <pid> Shutdown` 这个命令并不存在
 * （JDK 8 / 21 实测均报 Unknown diagnostic command），`taskkill` 不带 /F 发的
 * WM_CLOSE 又只对进程自己拥有的窗口有效。attach + agent 是 Windows 上唯一
 * 「应用零配置」的优雅停止通道。
 *
 * 兼容性（已实测）：class 版本 52，JDK 8 ~ 25 均可加载。
 * 启动器与目标 JVM **必须同主版本**（JDK 8 启动器打 JDK 21 目标、
 * JDK 21 启动器打 JDK 8 目标均实测失败）。
 * JDK 9 起 attach API 位于 jdk.attach 模块，无需 tools.jar；JDK 8 需要把
 * tools.jar 放进 classpath。
 */
public class AttachMain {
    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("usage: AttachMain <pid> <agent-jar>");
            System.exit(2);
        }
        VirtualMachine vm = VirtualMachine.attach(args[0]);
        try {
            vm.loadAgent(args[1]);
        } finally {
            vm.detach();
        }
    }
}
