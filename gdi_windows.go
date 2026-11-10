// Copyright (c) 2026 Greg and Contributors

package main

import "unsafe"

type viewer struct {
	hwnd   uintptr
	pixels []byte
	width  int
	height int
}

func newViewer(hwnd uintptr) *viewer {
	return &viewer{hwnd: hwnd}
}

func (v *viewer) paint() {
	var ps paintStruct
	hdc, _, _ := procBeginPaint.Call(v.hwnd, uintptr(unsafe.Pointer(&ps)))
	defer procEndPaint.Call(v.hwnd, uintptr(unsafe.Pointer(&ps)))

	var rect winRect
	if ok, _, _ := procGetClientRect.Call(v.hwnd, uintptr(unsafe.Pointer(&rect))); ok == 0 {
		return
	}

	width := int(rect.right - rect.left)
	height := int(rect.bottom - rect.top)
	if width <= 0 || height <= 0 {
		return
	}

	// COLORREF = 0x00BBGGRR; #1c2028 → R=0x1c G=0x20 B=0x28 → 0x28201c
	hBrush, _, _ := procCreateSolidBrush.Call(0x28201c)
	defer procDeleteObject.Call(hBrush)

	if len(v.pixels) == 0 || v.width <= 0 || v.height <= 0 {
		procFillRect.Call(hdc, uintptr(unsafe.Pointer(&rect)), hBrush)
		return
	}

	drawWidth, drawHeight := fitSize(v.width, v.height, width, height)
	if drawWidth <= 0 || drawHeight <= 0 {
		return
	}
	x := (width - drawWidth) / 2
	y := (height - drawHeight) / 2

	if x > 0 {
		r := winRect{left: 0, top: 0, right: int32(x), bottom: int32(height)}
		procFillRect.Call(hdc, uintptr(unsafe.Pointer(&r)), hBrush)
		r = winRect{left: int32(x + drawWidth), top: 0, right: int32(width), bottom: int32(height)}
		procFillRect.Call(hdc, uintptr(unsafe.Pointer(&r)), hBrush)
	}
	if y > 0 {
		r := winRect{left: 0, top: 0, right: int32(width), bottom: int32(y)}
		procFillRect.Call(hdc, uintptr(unsafe.Pointer(&r)), hBrush)
		r = winRect{left: 0, top: int32(y + drawHeight), right: int32(width), bottom: int32(height)}
		procFillRect.Call(hdc, uintptr(unsafe.Pointer(&r)), hBrush)
	}

	procSetStretchBltMode.Call(hdc, halftone)
	procSetBrushOrgEx.Call(hdc, 0, 0, 0)

	info := bitmapInfoHeader{
		size:      uint32(unsafe.Sizeof(bitmapInfoHeader{})),
		width:     int32(v.width),
		height:    -int32(v.height),
		planes:    1,
		bitCount:  32,
		sizeImage: uint32(len(v.pixels)),
	}

	procStretchDIBits.Call(
		hdc,
		uintptr(x), uintptr(y), uintptr(drawWidth), uintptr(drawHeight),
		0, 0, uintptr(v.width), uintptr(v.height),
		uintptr(unsafe.Pointer(&v.pixels[0])),
		uintptr(unsafe.Pointer(&info)),
		dibRGBColors,
		srcCopy,
	)
}

func fitSize(imageWidth, imageHeight, width, height int) (int, int) {
	if imageWidth <= 0 || imageHeight <= 0 || width <= 0 || height <= 0 {
		return 0, 0
	}
	drawWidth := width
	drawHeight := imageHeight * width / imageWidth
	if drawHeight > height {
		drawHeight = height
		drawWidth = imageWidth * height / imageHeight
	}
	return drawWidth, drawHeight
}
