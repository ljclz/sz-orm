package szorm

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// loadDLLForTest 将 sz_orm_go.dll 所在目录加入 DLL 搜索路径（Windows）
func loadDLLForTest(t *testing.T) {
	t.Helper()
	// 从 Rust target 目录加载（cargo build 产物）
	dir, err := filepath.Abs(filepath.Join("..", "..", "..", "target", "debug"))
	if err != nil {
		t.Fatalf("resolve target dir: %v", err)
	}
	// 将目录加入 PATH，使 syscall.LoadDLL 能找到
	old := os.Getenv("PATH")
	os.Setenv("PATH", dir+string(os.PathListSeparator)+old)
	t.Cleanup(func() { os.Setenv("PATH", old) })
}

func TestVersion(t *testing.T) {
	loadDLLForTest(t)
	v := Version()
	if v < 1 {
		t.Fatalf("version should be >= 1, got %d", v)
	}
	t.Logf("bind version = %d", v)
}

func TestPoolE2E(t *testing.T) {
	loadDLLForTest(t)

	pool, err := NewPool("sqlite::memory:", 4)
	if err != nil {
		t.Fatalf("NewPool: %v", err)
	}
	defer pool.Close()

	if !pool.Ping() {
		t.Fatal("pool should be healthy")
	}
	t.Log("pool created and healthy")

	if _, err := pool.Execute("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT)"); err != nil {
		t.Fatalf("create table: %v", err)
	}

	n, err := pool.Execute("INSERT INTO users (name) VALUES ('Alice'), ('Bob'), ('Carol')")
	if err != nil {
		t.Fatalf("insert: %v", err)
	}
	if n != 3 {
		t.Fatalf("expected 3 rows inserted, got %d", n)
	}
	t.Logf("inserted %d rows", n)

	json, err := pool.Query("SELECT id, name FROM users ORDER BY id")
	if err != nil {
		t.Fatalf("query: %v", err)
	}
	for _, name := range []string{"Alice", "Bob", "Carol"} {
		if !strings.Contains(json, name) {
			t.Fatalf("query result missing %q: %s", name, json)
		}
	}
	t.Logf("query json = %s", json)

	// 无效 SQL 应返回错误
	if _, err := pool.Query("SELECT * FROM nonexistent"); err == nil {
		t.Fatal("invalid SQL should return error")
	}

	if _, err := pool.Execute("INSERT INTO users (name) VALUES ('X')"); err != nil {
		t.Fatalf("second insert: %v", err)
	}

	pool.Close()
	if pool.Ping() {
		t.Fatal("closed pool should not be healthy")
	}
}
