// SZ-ORM Java 绑定端到端验证：真实 JVM 调用 JNI 绑定
package sz_orm_java;

/**
 * 端到端验证：创建 SQLite 内存连接池 → 建表 → 插入 → 查询 → 健康检查。
 *
 * 运行方式（需先构建 Rust cdylib）：
 *   cargo build -p sz-orm-java
 *   javac -encoding UTF-8 -d . java/sz_orm_java/SzOrmPool.java java-test/sz_orm_java/SzOrmPoolTest.java
 *   java -Djava.library.path=../../../target/debug sz_orm_java.SzOrmPoolTest
 */
public class SzOrmPoolTest {

    public static void main(String[] args) {
        // 1. 版本号
        int version = SzOrmPool.bindVersion();
        System.out.println("[1] bind version = " + version);
        if (version < 1) {
            throw new AssertionError("version should be >= 1");
        }

        // 2. 创建连接池（SQLite 内存库）
        SzOrmPool pool = SzOrmPool.create("sqlite::memory:", 4);
        System.out.println("[2] pool created");

        // 3. 健康检查
        boolean healthy = pool.isHealthy();
        System.out.println("[3] healthy = " + healthy);
        if (!healthy) {
            throw new AssertionError("pool should be healthy");
        }

        // 4. 建表 + 插入
        long create = pool.execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
        System.out.println("[4] create table rows = " + create);
        if (create < 0) {
            throw new AssertionError("create table failed");
        }
        long inserted = pool.execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob'), ('Carol')");
        System.out.println("[5] inserted = " + inserted);
        if (inserted != 3) {
            throw new AssertionError("expected 3 rows inserted, got " + inserted);
        }

        // 5. 查询并校验 JSON
        String json = pool.query("SELECT id, name FROM users ORDER BY id");
        System.out.println("[6] query json = " + json);
        if (!json.contains("Alice") || !json.contains("Bob") || !json.contains("Carol")) {
            throw new AssertionError("query result missing expected names: " + json);
        }

        // 6. 释放
        pool.close();
        System.out.println("[7] pool closed");

        System.out.println("SZ-ORM Java binding E2E: ALL PASSED");
    }
}
