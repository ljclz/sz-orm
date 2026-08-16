//go:build !windows

package szorm

import (
	"fmt"
	"unsafe"
)

// unixLibrary 基于 dlopen 的加载器（类 Unix 平台）
type unixLibrary struct {
	handle unsafe.Pointer
}

func newUnixLibrary(_name string) (*unixLibrary, error) {
	return nil, fmt.Errorf("szorm: unix dlopen not supported in this build (use cgo variant)")
}

func (u *unixLibrary) proc(_name string) (procFunc, error) {
	return nil, fmt.Errorf("szorm: unix dlopen not supported in this build")
}

// cStringToString 读取 NUL 结尾 C 字符串
func cStringToString(ptr uintptr) string {
	if ptr == 0 {
		return ""
	}
	var buf []byte
	for i := uintptr(0); ; i++ {
		b := *(*byte)(unsafe.Pointer(ptr + i))
		if b == 0 {
			break
		}
		buf = append(buf, b)
	}
	return string(buf)
}
