// Copyright (c) 2026 Greg and Contributors

package main

import (
	"fmt"
	"os"
	"syscall"
	"unsafe"
)

// openImageDialog asks for an image file; an empty result means the dialog
// was cancelled
func openImageDialog() string {
	file := make([]uint16, 32768)
	filter := utf16Filter(
		"Images (*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp)",
		"*.png;*.jpg;*.jpeg;*.gif;*.bmp;*.webp",
		"All files (*.*)",
		"*.*",
	)
	title, _ := syscall.UTF16PtrFromString("Open image")

	ofn := openFileNameW{
		length:      uint32(unsafe.Sizeof(openFileNameW{})),
		filter:      &filter[0],
		filterIndex: 1,
		file:        &file[0],
		maxFile:     uint32(len(file)),
		title:       title,
		flags:       ofnHideReadOnly | ofnNoChangeDir | ofnPathMustExist | ofnFileMustExist | ofnExplorer,
	}
	if ok, _, _ := procGetOpenFileNameW.Call(uintptr(unsafe.Pointer(&ofn))); ok == 0 {
		return ""
	}
	return syscall.UTF16ToString(file)
}

// utf16Filter joins dialog filter strings into the double-NUL-terminated list
// GetOpenFileNameW expects
func utf16Filter(parts ...string) []uint16 {
	var out []uint16
	for _, part := range parts {
		encoded, err := syscall.UTF16FromString(part)
		if err != nil {
			continue
		}
		out = append(out, encoded...)
	}
	return append(out, 0)
}

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
