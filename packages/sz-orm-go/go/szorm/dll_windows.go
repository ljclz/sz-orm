//go:build windows

package szorm

import (
	"syscall"
	"unsafe"
)

// windowsDLL 基于 syscall.LoadDLL 的加载器
type windowsDLL struct {
	dll *syscall.DLL
}

func newWindowsDLL(name string) (*windowsDLL, error) {
	dll, err := syscall.LoadDLL(name)
	if err != nil {
		return nil, err
	}
	return &windowsDLL{dll: dll}, nil
}

func (w *windowsDLL) proc(name string) (procFunc, error) {
	p, err := w.dll.FindProc(name)
	if err != nil {
		return nil, err
	}
	return func(args ...uintptr) (uintptr, error) {
		r1, _, _ := p.Call(args...)
		// 注意：Rust 导出的 C ABI 不使用 Windows GetLastError 约定，
		// 成败以返回值本身判断（句柄/错误码），忽略陈旧的 last error。
		return r1, nil
	}, nil
}

// cStringToString 读取 Rust 侧返回的 NUL 结尾 C 字符串
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
