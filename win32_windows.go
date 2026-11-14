// Copyright (c) 2026 Greg and Contributors

package main

import "syscall"

const (
	// window styles
	wsOverlappedWindow = 0x00CF0000
	wsPopup            = 0x80000000
	wsVisible          = 0x10000000

	// class styles
	csHRedraw = 0x0002
	csVRedraw = 0x0001

	// window messages
	wmDestroy        = 0x0002
	wmSize           = 0x0005
	wmPaint          = 0x000F
	wmSetCursor      = 0x0020
	wmEraseBkgnd     = 0x0014
	wmClose          = 0x0010
	wmKeyDown        = 0x0100
	wmMouseMove      = 0x0200
	wmLButtonDown    = 0x0201
	wmLButtonUp      = 0x0202
	wmRButtonDown    = 0x0204
	wmRButtonUp      = 0x0205
	wmMouseWheel     = 0x020A
	wmCaptureChanged = 0x0215
	wmUserFrame      = 0x0400 + 1 // WM_USER+1
	wmUserAnim       = 0x0400 + 2 // WM_USER+2

	// WM_SIZE
	sizeMinimized = 1

	// virtual keys
	vkPrior  = 0x21
	vkNext   = 0x22
	vkLeft   = 0x25
	vkRight  = 0x27
	vkSpace  = 0x20
	vkEscape = 0x1B
	vkF11    = 0x7A

	// SetWindowPos
	swpNoSize       = 0x0001
	swpNoMove       = 0x0002
	swpNoZOrder     = 0x0004
	swpFrameChanged = 0x0020
	swpShowWindow   = 0x0040

	// hit testing
	htClient = 1

	// DrawText flags
	dtCenter    = 0x0001
	dtWordBreak = 0x0010
	dtCalcRect  = 0x0400

	// ShowWindow
	swShow = 5

	// GetWindowLongPtr index — typed so uintptr cast doesn't overflow
	gwlUserdata uintptr = ^uintptr(20) // -21 as two's complement uintptr
	gwlStyle    uintptr = ^uintptr(15) // -16 as two's complement uintptr

	// system colors
	colorWindow = 5

	// GDI stretch mode
	halftone     = 4
	dibRGBColors = 0
	srcCopy      = 0x00CC0020

	// AlphaBlend format: source pixels carry their own alpha
	acSrcAlpha = 1

	// cursor resource
	idcArrow   = 32512
	idcSizeAll = 32646
	idcHand    = 32649

	// stock objects
	defaultGuiFont = 17

	// SetBkMode
	bkTransparent = 1

	// palette
	navButtonColor = 0x003c312c
	navArrowColor  = 0x00efe9e6
	messageColor   = 0x00e0e0e0

	// MonitorFromPoint
	monitorDefaultToPrimary = 1
	monitorDefaultToNearest = 2

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
	procSetCapture         = user32.NewProc("SetCapture")
	procReleaseCapture     = user32.NewProc("ReleaseCapture")
	procSetCursor          = user32.NewProc("SetCursor")
	procScreenToClient     = user32.NewProc("ScreenToClient")
	procAdjustWindowRect   = user32.NewProc("AdjustWindowRect")
	procLoadCursorW        = user32.NewProc("LoadCursorW")
	procMonitorFromPoint   = user32.NewProc("MonitorFromPoint")
	procMonitorFromWindow  = user32.NewProc("MonitorFromWindow")
	procGetMonitorInfoW    = user32.NewProc("GetMonitorInfoW")
	procSetWindowTextW     = user32.NewProc("SetWindowTextW")
	procGetCursorPos       = user32.NewProc("GetCursorPos")
	procDrawTextW          = user32.NewProc("DrawTextW")
	procSetWindowPos       = user32.NewProc("SetWindowPos")
	procGetWindowPlacement = user32.NewProc("GetWindowPlacement")
	procSetWindowPlacement = user32.NewProc("SetWindowPlacement")

	procPatBlt                 = gdi32.NewProc("PatBlt")
	procBitBlt                 = gdi32.NewProc("BitBlt")
	procCreateCompatibleDC     = gdi32.NewProc("CreateCompatibleDC")
	procCreateCompatibleBitmap = gdi32.NewProc("CreateCompatibleBitmap")
	procSelectObject           = gdi32.NewProc("SelectObject")
	procDeleteDC               = gdi32.NewProc("DeleteDC")
	procSetStretchBltMode      = gdi32.NewProc("SetStretchBltMode")
	procSetBrushOrgEx          = gdi32.NewProc("SetBrushOrgEx")
	procStretchDIBits          = gdi32.NewProc("StretchDIBits")
	procGetDC                  = user32.NewProc("GetDC")
	procReleaseDC              = user32.NewProc("ReleaseDC")
	procCreateSolidBrush       = gdi32.NewProc("CreateSolidBrush")
	procFillRect               = user32.NewProc("FillRect")
	procSetTextColor           = gdi32.NewProc("SetTextColor")
	procSetBkMode              = gdi32.NewProc("SetBkMode")
	procGetStockObject         = gdi32.NewProc("GetStockObject")
	procCreateBitmap           = gdi32.NewProc("CreateBitmap")
	procDeleteObject           = gdi32.NewProc("DeleteObject")
	procCreateIconIndirect     = user32.NewProc("CreateIconIndirect")
	procCreateDIBSection       = gdi32.NewProc("CreateDIBSection")

	msimg32                   = syscall.NewLazyDLL("msimg32.dll")
	procAlphaBlend            = msimg32.NewProc("AlphaBlend")
	dwmapi                    = syscall.NewLazyDLL("dwmapi.dll")
	procDwmSetWindowAttribute = dwmapi.NewProc("DwmSetWindowAttribute")
	winmm                     = syscall.NewLazyDLL("winmm.dll")
	procTimeBeginPeriod       = winmm.NewProc("timeBeginPeriod")
	procTimeEndPeriod         = winmm.NewProc("timeEndPeriod")
)

var (
	// HWND_TOPMOST/HWND_NOTOPMOST are negative handles passed as integers
	hwndTopmost    = ^uintptr(0)
	hwndNotTopmost = ^uintptr(1)
)

type winRect struct {
	left   int32
	top    int32
	right  int32
	bottom int32
}

type windowPlacement struct {
	length           uint32
	flags            uint32
	showCmd          uint32
	ptMinPosition    point
	ptMaxPosition    point
	rcNormalPosition winRect
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

type point struct {
	x int32
	y int32
}

type iconInfo struct {
	fIcon    uint32
	xHotspot uint32
	yHotspot uint32
	hbmMask  uintptr
	hbmColor uintptr
}
