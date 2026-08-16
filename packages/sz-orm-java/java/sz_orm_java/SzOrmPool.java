// SZ-ORM Java 绑定：SzOrmPool — 连接池 + 查询
package sz_orm_java;

/**
 * SZ-ORM 连接池的 Java 封装。
 *
 * 通过 JNI 调用 sz-orm-cabi 的 C ABI（SQLite 后端），
 * 提供真实可用的连接池与查询能力。
 */
public class SzOrmPool implements AutoCloseable {

    static {
        // 加载 JNI 实现（sz_orm_java.dll / libsz_orm_java.so）
        // 运行时通过 -Djava.library.path 指定 dll 所在目录
        System.loadLibrary("sz_orm_java");
    }

    // 本地方法（JNI 实现位于 sz_orm_java Rust crate）
    private static native long poolNew(String dsn, int maxConnections);
    private static native void poolFree(long handle);
    private static native int ping(long handle);
    private static native String query(long handle, String sql);
    private static native long execute(long handle, String sql);
    private static native int version();

    /** 持有 Rust 侧连接池句柄（0 表示未创建） */
    private long handle;

    private SzOrmPool(long handle) {
        this.handle = handle;
    }

    /**
     * 创建连接池（SQLite 后端）。
     *
     * @param dsn SQLite 连接串，如 "sqlite::memory:" 或 "sqlite://path/to.db"
     * @param maxConnections 最大连接数
     * @return 连接池实例
     * @throws IllegalStateException 创建失败时
     */
    public static SzOrmPool create(String dsn, int maxConnections) {
        long h = poolNew(dsn, maxConnections);
        if (h == 0) {
            throw new IllegalStateException("SZ-ORM pool creation failed for dsn: " + dsn);
        }
        return new SzOrmPool(h);
    }

    /** 健康检查（真实 acquire + ping） */
    public boolean isHealthy() {
        return ping(handle) == 1;
    }

    /**
     * 执行查询，返回 JSON 行数组（"[{\"col\":val},...]"）。
     *
     * @throws IllegalStateException 查询失败时
     */
    public String query(String sql) {
        String json = query(handle, sql);
        if (json == null) {
            throw new IllegalStateException("SZ-ORM query failed: " + sql);
        }
        return json;
    }

    /** 执行写语句，返回影响行数（<0 表示失败） */
    public long execute(String sql) {
        return execute(handle, sql);
    }

    /** 绑定版本号 */
    public static int bindVersion() {
        return version();
    }

    @Override
    public void close() {
        if (handle != 0) {
            poolFree(handle);
            handle = 0;
        }
    }
}
