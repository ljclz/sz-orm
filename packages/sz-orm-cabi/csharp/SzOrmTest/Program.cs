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

// 5. 错误路径：不存在的表应抛出带错误码的异常
try
{
    pool.Query("SELECT * FROM nonexistent_table");
    Console.Error.WriteLine("FAIL: expected exception for invalid SQL");
    Environment.Exit(1);
}
catch (InvalidOperationException ex)
{
    Console.WriteLine($"Expected error handled: {ex.Message}");
}

Console.WriteLine("PASS: SZ-ORM C# binding end-to-end test succeeded");