// SZ-ORM C# 绑定端到端测试
// 前置：cargo build -p sz-orm-cabi（生成 target/debug/sz_orm_cabi.dll）

using SzOrm;

Console.WriteLine($"SZ-ORM C# binding test, bind version {SzOrmPool.BindVersion()}");

// 1. 创建 SQLite 内存连接池
using var pool = SzOrmPool.Create("sqlite::memory:", 4);
Console.WriteLine($"Pool healthy: {pool.IsHealthy()}");

// 2. 建表 + 插入
pool.Execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)");
ulong inserted = pool.Execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob'), ('Carol')");
Console.WriteLine($"Inserted rows: {inserted}");
if (inserted != 3)
{
    Console.Error.WriteLine($"FAIL: expected 3 inserted, got {inserted}");
    Environment.Exit(1);
}

// 3. 查询
string json = pool.Query("SELECT id, name FROM users ORDER BY id");
Console.WriteLine($"Query result: {json}");
if (!json.Contains("Alice") || !json.Contains("Bob") || !json.Contains("Carol"))
{
    Console.Error.WriteLine($"FAIL: query result missing rows: {json}");
    Environment.Exit(1);
}

// 4. UTF-8 往返：非 ASCII 数据必须原样返回
//    （C ABI 约定 UTF-8；绑定侧用 LPUTF8Str / PtrToStringUTF8）
pool.Execute("CREATE TABLE zh (id INTEGER PRIMARY KEY, name TEXT)");
ulong zhInserted = pool.Execute("INSERT INTO zh (name) VALUES ('张三'), ('李四')");
if (zhInserted != 2)
{
    Console.Error.WriteLine($"FAIL: expected 2 inserted Chinese rows, got {zhInserted}");
    Environment.Exit(1);
}
string zhJson = pool.Query("SELECT name FROM zh ORDER BY id");
Console.WriteLine($"UTF-8 round-trip: {zhJson}");
if (!zhJson.Contains("张三") || !zhJson.Contains("李四"))
{
    Console.Error.WriteLine($"FAIL: UTF-8 round-trip broken: {zhJson}");
    Environment.Exit(1);
}

// 5. 错误路径：不存在的表应抛出强类型异常（SzOrmQueryException : SzOrmException，
//    2026-10-28 P3-2：取代统一 InvalidOperationException）
try
{
    pool.Query("SELECT * FROM nonexistent_table");
    Console.Error.WriteLine("FAIL: expected exception for invalid SQL");
    Environment.Exit(1);
}
catch (SzOrmQueryException ex)
{
    Console.WriteLine($"Expected error handled: code={ex.ErrorCode}, {ex.Message}");
}

// 6. 错误文本通道：异常消息必须带上数据库原文（而不只是"query execution failed"）
if (string.IsNullOrEmpty(SzOrmPool.LastErrorMessage()))
{
    Console.Error.WriteLine("FAIL: expected detailed last error message");
    Environment.Exit(1);
}
if (!SzOrmPool.LastErrorMessage()!.Contains("nonexistent_table"))
{
    Console.Error.WriteLine($"FAIL: last error should name the table: {SzOrmPool.LastErrorMessage()}");
    Environment.Exit(1);
}
Console.WriteLine($"Last error detail: {SzOrmPool.LastErrorMessage()}");

// 7. 参数化接口：注入串作为值绑定，恒真串不得匹配任何行
pool.Execute("CREATE TABLE inj (id INTEGER PRIMARY KEY, name TEXT)");
string payload = "O'Brien\"; DROP TABLE inj; --";
pool.ExecuteParams("INSERT INTO inj (name) VALUES (?)", payload);
string injJson = pool.QueryParams("SELECT name FROM inj WHERE name = ?", payload);
if (!injJson.Contains("DROP TABLE inj"))
{
    Console.Error.WriteLine($"FAIL: payload must round-trip as data: {injJson}");
    Environment.Exit(1);
}
string tautology = pool.QueryParams("SELECT name FROM inj WHERE name = ?", "' OR '1'='1");
if (tautology != "[]")
{
    Console.Error.WriteLine($"FAIL: tautology payload must not match rows: {tautology}");
    Environment.Exit(1);
}
string countJson = pool.Query("SELECT COUNT(*) AS c FROM inj");
if (!countJson.Contains("1"))
{
    Console.Error.WriteLine("FAIL: table should still exist after injection attempt");
    Environment.Exit(1);
}
Console.WriteLine($"Injection payload stayed inert: {injJson}");

// 8. 类型保真：decimal 38 位、DateTime、DBNull 往返
pool.Execute("CREATE TABLE typed (id INTEGER PRIMARY KEY, amount TEXT, at TEXT, note TEXT)");
const string exactDecimal = "12345678901234567890123456789012345678";
pool.ExecuteParams(
    "INSERT INTO typed (id, amount, at, note) VALUES (?, ?, ?, ?)",
    1,
    SzOrmParam.DecimalString(exactDecimal),
    new DateTime(2026, 10, 28, 12, 34, 56, 123, DateTimeKind.Utc),
    DBNull.Value);

string typedJson = pool.QueryParams(
    "SELECT amount, at, note FROM typed WHERE id = ? AND note IS NULL", 1);
if (!typedJson.Contains(exactDecimal))
{
    Console.Error.WriteLine($"FAIL: 38-digit decimal lost precision: {typedJson}");
    Environment.Exit(1);
}
if (!typedJson.Contains("2026-10-28T12:34:56.123"))
{
    Console.Error.WriteLine($"FAIL: DateTime round-trip broken: {typedJson}");
    Environment.Exit(1);
}
Console.WriteLine($"Typed round-trip ok: {typedJson}");

// 9. 行值解码器：从行 JSON 还原 decimal / DateTime / NULL
using (var rows = System.Text.Json.JsonDocument.Parse(typedJson))
{
    var first = rows.RootElement[0];
    string? amount = SzOrmValue.GetDecimalString(first.GetProperty("amount"));
    if (amount != exactDecimal)
    {
        Console.Error.WriteLine($"FAIL: SzOrmValue.GetDecimalString got {amount ?? "<null>"}");
        Environment.Exit(1);
    }
    if (SzOrmValue.GetDateTime(first.GetProperty("at")) is not DateTime dt ||
        dt.Year != 2026 || dt.Month != 10 || dt.Day != 28)
    {
        Console.Error.WriteLine("FAIL: SzOrmValue.GetDateTime failed");
        Environment.Exit(1);
    }
    if (!SzOrmValue.IsNull(first.GetProperty("note")))
    {
        Console.Error.WriteLine("FAIL: SzOrmValue.IsNull should be true for NULL column");
        Environment.Exit(1);
    }
}

Console.WriteLine("PASS: SZ-ORM C# binding end-to-end test succeeded");

// ===== 2026-10-28 P3-2 扩展用例：事务 / 批量 / 统计 =====

// 10. 事务：提交路径（两条写语句原子生效）
pool.Execute("CREATE TABLE tx_t (id INTEGER PRIMARY KEY, name TEXT)");
using (var tx = pool.BeginTransaction())
{
    tx.Execute("INSERT INTO tx_t (name) VALUES ('tx_a')");
    tx.Execute("INSERT INTO tx_t (name) VALUES ('tx_b')");
    tx.Commit();
}
string committedJson = pool.Query("SELECT name FROM tx_t ORDER BY id");
if (!committedJson.Contains("tx_a") || !committedJson.Contains("tx_b"))
{
    Console.Error.WriteLine($"FAIL: transaction commit lost rows: {committedJson}");
    Environment.Exit(1);
}
Console.WriteLine("Transaction commit ok");

// 11. 事务：回滚路径（写入必须不可见）
using (var tx = pool.BeginTransaction())
{
    tx.Execute("INSERT INTO tx_t (name) VALUES ('tx_rolled_back')");
    tx.Rollback();
}
string rolledJson = pool.Query("SELECT COUNT(*) AS c FROM tx_t");
if (rolledJson.Contains("tx_rolled_back") || rolledJson.Contains("\"c\":2"))
{
    Console.Error.WriteLine($"FAIL: rollback did not discard write: {rolledJson}");
    Environment.Exit(1);
}
Console.WriteLine("Transaction rollback ok");

// 12. 事务：Dispose 未提交 → 隐式回滚（Rust 侧 transaction_free 语义）
using (var tx = pool.BeginTransaction())
{
    tx.Execute("INSERT INTO tx_t (name) VALUES ('tx_disposed')");
    // 不提交、不回滚，直接 Dispose
}
string disposedJson = pool.Query("SELECT name FROM tx_t");
if (disposedJson.Contains("tx_disposed"))
{
    Console.Error.WriteLine($"FAIL: disposed transaction should roll back implicitly: {disposedJson}");
    Environment.Exit(1);
}
Console.WriteLine("Transaction dispose-rollback ok");

// 13. 批量执行：分号分隔多语句
ulong batchRows = pool.ExecuteBatch("INSERT INTO tx_t (name) VALUES ('batch_1'); INSERT INTO tx_t (name) VALUES ('batch_2');");
if (batchRows < 2)
{
    Console.Error.WriteLine($"FAIL: execute_batch expected >=2 rows, got {batchRows}");
    Environment.Exit(1);
}
Console.WriteLine($"ExecuteBatch ok: {batchRows} rows");

// 14. 池统计与累计指标：数值自洽（created >= acquired 不可判，仅验证字段可读且合理）
var stats = pool.GetStats();
var metrics = pool.GetMetrics();
if (stats.Max < stats.Active || stats.Max == 0)
{
    Console.Error.WriteLine($"FAIL: pool stats inconsistent: max={stats.Max}, active={stats.Active}");
    Environment.Exit(1);
}
if (metrics.AcquireCount == 0 || metrics.ConnectionCreatedCount == 0)
{
    Console.Error.WriteLine("FAIL: pool metrics should count acquires and creations");
    Environment.Exit(1);
}
Console.WriteLine($"Pool stats ok: idle={stats.Idle}, active={stats.Active}, max={stats.Max}; " +
                  $"acquires={metrics.AcquireCount}, created={metrics.ConnectionCreatedCount}");

Console.WriteLine("PASS: SZ-ORM C# binding extended E2E (P3-2) succeeded");