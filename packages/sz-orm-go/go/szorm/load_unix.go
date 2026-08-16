//go:build !windows

package szorm

import "fmt"

func loadLibrary() {
	lib, err := newUnixLibrary("libsz_orm_go.so")
	if err != nil {
		loadErr = fmt.Errorf("szorm: load library: %w", err)
		return
	}
	for _, name := range []string{
		"sz_orm_go_pool_new", "sz_orm_go_pool_free", "sz_orm_go_ping",
		"sz_orm_go_query", "sz_orm_go_string_free", "sz_orm_go_execute", "sz_orm_go_result_free",
		"sz_orm_go_version",
	} {
		p, err := lib.proc(name)
		if err != nil {
			loadErr = fmt.Errorf("szorm: resolve %s: %w", name, err)
			return
		}
		procs[name] = p
	}
}
