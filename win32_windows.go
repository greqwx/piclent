// Copyright (c) 2026 Greg and Contributors

package main

import "syscall"

const (
	// window styles
	wsOverlappedWindow = 0x00CF0000

	// class styles
	csHRedraw = 0x0002
	csVRedraw = 0x0001

	// window messages
	wmDestroy   = 0x0002
	wmSize      = 0x0005
	wmPaint     = 0x000F
	wmEraseBkgnd = 0x0014
	wmClose     = 0x0010
	wmUserFrame = 0x0400 + 1 // WM_USER+1

	// ShowWindow
	swShow = 5

	// GetWindowLongPtr index — typed so uintptr cast doesn't overflow
	gwlUserdata uintptr = ^uintptr(20) // -21 as two's complement uintptr

	// system colors
	colorWindow = 5

	// GDI stretch mode
	halftone     = 4
	dibRGBColors = 0
	srcCopy      = 0x00CC0020

	// cursor resource
	idcArrow = 32512

	// MonitorFromPoint
	monitorDefaultToPrimary = 1

	// DwmSetWindowAttribute
	dwmwaUseImmersiveDarkMode = 20
)

var (
	user32   = syscall.NewLazyDLL("user32.dll")
	kernel32 = syscall.NewLazyDLL("kernel32.dll")
	gdi32    = syscall.NewLazyDLL("gdi32.dll")

	procGetModuleHandleW   = kernel32.NewProc("GetModuleHandleW")
	procGetConsoleWindow   = kernel32.NewProc("GetConsoleWindow")
	procMessageBoxW        = user32.NewProc("MessageBoxW")
	procRegisterClassExW   = user32.NewProc("RegisterClassExW")
	procCreateWindowExW    = user32.NewProc("CreateWindowExW")
	procShowWindow         = user32.NewProc("ShowWindow")
	procUpdateWindow       = user32.NewProc("UpdateWindow")
	procDefWindowProcW     = user32.NewProc("DefWindowProcW")
	procPostQuitMessage    = user32.NewProc("PostQuitMessage")
	procGetMessageW        = user32.NewProc("GetMessageW")
	procTranslateMessage   = user32.NewProc("TranslateMessage")
	procDispatchMessageW   = user32.NewProc("DispatchMessageW")
	procPostMessageW       = user32.NewProc("PostMessageW")
	procGetClientRect      = user32.NewProc("GetClientRect")
	procInvalidateRect     = user32.NewProc("InvalidateRect")
	procBeginPaint         = user32.NewProc("BeginPaint")
	procEndPaint           = user32.NewProc("EndPaint")
	procSetWindowLongPtrW  = user32.NewProc("SetWindowLongPtrW")
	procGetWindowLongPtrW  = user32.NewProc("GetWindowLongPtrW")
	procAdjustWindowRect   = user32.NewProc("AdjustWindowRect")
	procLoadCursorW        = user32.NewProc("LoadCursorW")
	procMonitorFromPoint   = user32.NewProc("MonitorFromPoint")
	procGetMonitorInfoW    = user32.NewProc("GetMonitorInfoW")

	procPatBlt             = gdi32.NewProc("PatBlt")
	procSetStretchBltMode  = gdi32.NewProc("SetStretchBltMode")
	procSetBrushOrgEx      = gdi32.NewProc("SetBrushOrgEx")
	procStretchDIBits      = gdi32.NewProc("StretchDIBits")
	procGetDC              = user32.NewProc("GetDC")
	procReleaseDC          = user32.NewProc("ReleaseDC")
	procCreateSolidBrush   = gdi32.NewProc("CreateSolidBrush")
	procFillRect           = user32.NewProc("FillRect")
	procCreateBitmap       = gdi32.NewProc("CreateBitmap")
	procDeleteObject       = gdi32.NewProc("DeleteObject")
	procCreateIconIndirect = user32.NewProc("CreateIconIndirect")

	dwmapi                       = syscall.NewLazyDLL("dwmapi.dll")
	procDwmSetWindowAttribute    = dwmapi.NewProc("DwmSetWindowAttribute")
)

type winRect struct {
	left   int32
	top    int32
	right  int32
	bottom int32
}

type wndClassExW struct {
	cbSize        uint32
	style         uint32
	lpfnWndProc   uintptr
	cbClsExtra    int32
	cbWndExtra    int32
	hInstance     uintptr
	hIcon         uintptr
	hCursor       uintptr
	hbrBackground uintptr
	lpszMenuName  *uint16
	lpszClassName *uint16
	hIconSm       uintptr
}

type msgW struct {
	hwnd    uintptr
	message uint32
	wParam  uintptr
	lParam  uintptr
	time    uint32
	pt      struct{ x, y int32 }
	_       uint32
}

// PAINTSTRUCT — 64 bytes, rest is reserved
type paintStruct struct {
	hdc         uintptr
	fErase      int32
	rcPaint     winRect
	fRestore    int32
	fIncUpdate  int32
	rgbReserved [32]byte
}

type bitmapInfoHeader struct {
	size          uint32
	width         int32
	height        int32
	planes        uint16
	bitCount      uint16
	compression   uint32
	sizeImage     uint32
	xPelsPerMeter int32
	yPelsPerMeter int32
	clrUsed       uint32
	clrImportant  uint32
}

type monitorInfo struct {
	cbSize    uint32
	rcMonitor winRect
	rcWork    winRect
	dwFlags   uint32
}

type iconInfo struct {
	fIcon    uint32
	xHotspot uint32
	yHotspot uint32
	hbmMask  uintptr
	hbmColor uintptr
}
