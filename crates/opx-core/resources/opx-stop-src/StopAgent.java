import java.lang.instrument.Instrumentation;

/**
 * opx 优雅停止 agent：被注入目标 JVM 后，让 JVM 自己走正常关闭流程退出。
 *
 * 为什么不直接调用 System.exit：agentmain 执行时 loadAgent 尚未返回，
 * 目标 JVM 会在 attach 协议栈中途退出，启动器侧会收到异常。
 * 因此另起一个非守护线程、稍等片刻再退出——这样 loadAgent 能正常返回。
 *
 * 被 JDK 21+ 注入时，目标 JVM 的 stderr 会出现 4 行
 * "WARNING: A Java agent has been loaded dynamically"（JEP 451），属预期。
 */
public class StopAgent {
    public static void agentmain(String args, Instrumentation inst) {
        Thread t = new Thread(() -> {
            try {
                Thread.sleep(200);
            } catch (InterruptedException ignored) {
                // 被中断也继续退出，保证语义
            }
            System.exit(0);
        }, "opx-graceful-stop");
        t.setDaemon(false);
        t.start();
    }
}
