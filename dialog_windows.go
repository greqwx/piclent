// Copyright (c) 2026 Greg and Contributors

package main

import (
	"fmt"
	"os"
	"syscall"
	"unsafe"
)

func fatal(format string, args ...any) {
	message := fmt.Sprintf(format, args...)
	fmt.Fprintln(os.Stderr, message)

	if console, _, _ := procGetConsoleWindow.Call(); console != 0 {
		return
	}

	text, err := syscall.UTF16PtrFromString(message)
	if err != nil {
		return
	}
	title, err := syscall.UTF16PtrFromString("piclent")
	if err != nil {
		return
	}

	const mbIconError = 0x00000010
	procMessageBoxW.Call(0, uintptr(unsafe.Pointer(text)), uintptr(unsafe.Pointer(title)), mbIconError)
}
