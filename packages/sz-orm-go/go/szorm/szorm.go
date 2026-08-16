// Package szorm 提供 SZ-ORM 的 Go 绑定。
//
// 通过动态库加载（Windows: syscall.LoadDLL；类 Unix: dlopen）调用
// sz-orm-go 的 C ABI（SQLite 后端），提供真实可用的连接池与查询。
//
// 不依赖 cgo，纯 Go 实现，跨平台可用。
package szorm

import (
	"fmt"
	"sync"
	"unsafe"
)

// dll 加载器抽象（Windows / 类 Unix 双实现）
type dllLoader interface {
	proc(name string) (procFunc, error)
}

type procFunc func(args ...uintptr) (uintptr, error)

var (
	loadOnce sync.Once
	loadErr  error
	procs    = map[string]procFunc{}
)

func init() {
	loadOnce.Do(loadLibrary)
}

// loadLibrary 由各平台文件提供（load_windows.go / load_unix.go）

// PoolConfigC 与 Rust 侧 PoolConfigC 布局一致（repr(C)）。
type PoolConfigC struct {
	MaxConnections   uint32
	MinConnections   uint32
	ConnectTimeoutMs uint64
	IdleTimeoutMs    uint64
}

// QueryResultC 与 Rust 侧 QueryResultC 布局一致（repr(C)）。
type QueryResultC struct {
	Success      int32
	ErrorCode    int32
	RowsAffected uint64
	LastInsertID uint64
}

// Pool 是 SZ-ORM 连接池的 Go 封装（持有 Rust 侧句柄）。
type Pool struct {
	handle uintptr
}

// NewPool 创建连接池（SQLite 后端，真实创建）。
//
// dsn 示例："sqlite::memory:" 或 "sqlite://path/to/db.sqlite"
func NewPool(dsn string, maxConnections uint32) (*Pool, error) {
	if loadErr != nil {
		return nil, loadErr
	}
	cfg := PoolConfigC{
		MaxConnections:   maxConnections,
		MinConnections:   1,
		ConnectTimeoutMs: 5000,
		IdleTimeoutMs:    300000,
	}
	dsnBytes := append([]byte(dsn), 0)
	dsnPtr := unsafe.Pointer(&dsnBytes[0])
	h, err := procs["sz_orm_go_pool_new"](uintptr(dsnPtr), uintptr(unsafe.Pointer(&cfg)))
	if err != nil {
		return nil, fmt.Errorf("szorm: pool_new: %w", err)
	}
	if h == 0 {
		return nil, fmt.Errorf("szorm: pool creation failed for dsn %q", dsn)
	}
	return &Pool{handle: h}, nil
}

// Ping 健康检查（真实 acquire + ping）。
func (p *Pool) Ping() bool {
	if p == nil || p.handle == 0 || loadErr != nil {
		return false
	}
	r, err := procs["sz_orm_go_ping"](p.handle)
	if err != nil {
		return false
	}
	return r == 1
}

// Query 执行查询，返回 JSON 行数组字符串。
func (p *Pool) Query(sql string) (string, error) {
	if p == nil || p.handle == 0 {
		return "", fmt.Errorf("szorm: pool is closed")
	}
	if loadErr != nil {
		return "", loadErr
	}
	sqlBytes := append([]byte(sql), 0)
	sqlPtr := unsafe.Pointer(&sqlBytes[0])
	jsonPtr, err := procs["sz_orm_go_query"](p.handle, uintptr(sqlPtr))
	if err != nil {
		return "", fmt.Errorf("szorm: query: %w", err)
	}
	if jsonPtr == 0 {
		return "", fmt.Errorf("szorm: query failed: %s", sql)
	}
	defer procs["sz_orm_go_string_free"](jsonPtr)
	return cStringToString(jsonPtr), nil
}

// Execute 执行写语句，返回影响行数（<0 表示失败）。
func (p *Pool) Execute(sql string) (int64, error) {
	if p == nil || p.handle == 0 {
		return -1, fmt.Errorf("szorm: pool is closed")
	}
	if loadErr != nil {
		return -1, loadErr
	}
	sqlBytes := append([]byte(sql), 0)
	sqlPtr := unsafe.Pointer(&sqlBytes[0])
	resPtr, err := procs["sz_orm_go_execute"](p.handle, uintptr(sqlPtr))
	if err != nil {
		return -1, fmt.Errorf("szorm: execute: %w", err)
	}
	if resPtr == 0 {
		return -1, fmt.Errorf("szorm: execute failed")
	}
	defer procs["sz_orm_go_result_free"](resPtr)
	res := *(*QueryResultC)(unsafe.Pointer(resPtr))
	if res.Success == 0 {
		return -1, fmt.Errorf("szorm: execute failed, code=%d", res.ErrorCode)
	}
	return int64(res.RowsAffected), nil
}

// Close 释放连接池。
func (p *Pool) Close() {
	if p != nil && p.handle != 0 {
		procs["sz_orm_go_pool_free"](p.handle)
		p.handle = 0
	}
}

// Version 返回绑定版本号。
func Version() uint32 {
	if loadErr != nil {
		return 0
	}
	r, _ := procs["sz_orm_go_version"]()
	return uint32(r)
}
