// Copyright (c) 2026 Greg and Contributors

package main

import (
	"math"
	"unsafe"
)

const (
	minZoom         = 1.0
	maxZoom         = 8.0
	wheelZoomFactor = 1.2
)

type viewer struct {
	hwnd   uintptr
	pixels []byte
	width  int
	height int

	zoom    float64
	offsetX float64
	offsetY float64

	dragging bool
	dragX    int
	dragY    int

	memDC     uintptr
	memBitmap uintptr
	memOld    uintptr
	memWidth  int
	memHeight int
}

type viewGeometry struct {
	x      int
	y      int
	width  int
	height int
	scale  float64
}

func newViewer(hwnd uintptr) *viewer {
	return &viewer{hwnd: hwnd, zoom: minZoom}
}

// layout returns where the image lands in the client area, clamping the pan
func (v *viewer) layout(clientWidth, clientHeight int) viewGeometry {
	if v.zoom < minZoom {
		v.zoom = minZoom
	}
	fitWidth, fitHeight := fitSize(v.width, v.height, clientWidth, clientHeight)
	if fitWidth <= 0 || fitHeight <= 0 {
		return viewGeometry{}
	}

	scale := float64(fitWidth) / float64(v.width) * v.zoom
	drawWidth := max(int(math.Round(float64(v.width)*scale)), 1)
	drawHeight := max(int(math.Round(float64(v.height)*scale)), 1)

	v.offsetX = clampOffset(v.offsetX, drawWidth, clientWidth)
	v.offsetY = clampOffset(v.offsetY, drawHeight, clientHeight)

	return viewGeometry{
		x:      (clientWidth-drawWidth)/2 + int(math.Round(v.offsetX)),
		y:      (clientHeight-drawHeight)/2 + int(math.Round(v.offsetY)),
		width:  drawWidth,
		height: drawHeight,
		scale:  scale,
	}
}

func clampOffset(offset float64, drawSize, clientSize int) float64 {
	if drawSize <= clientSize {
		return 0
	}
	limit := float64(drawSize-clientSize) / 2
	return math.Min(math.Max(offset, -limit), limit)
}

// zoomWheel zooms around the cursor, keeping the point under it in place
func (v *viewer) zoomWheel(screenX, screenY, delta int) {
	if delta == 0 {
		return
	}
	var pt point
	pt.x = int32(screenX)
	pt.y = int32(screenY)
	procScreenToClient.Call(v.hwnd, uintptr(unsafe.Pointer(&pt)))
	v.zoomAt(int(pt.x), int(pt.y), math.Pow(wheelZoomFactor, float64(delta)/120))
}

func (v *viewer) zoomAt(cursorX, cursorY int, factor float64) {
	if len(v.pixels) == 0 || v.width <= 0 || v.height <= 0 {
		return
	}
	var rect winRect
	if ok, _, _ := procGetClientRect.Call(v.hwnd, uintptr(unsafe.Pointer(&rect))); ok == 0 {
		return
	}
	width := int(rect.right - rect.left)
	height := int(rect.bottom - rect.top)
	if width <= 0 || height <= 0 {
		return
	}

	current := v.layout(width, height)
	if current.scale <= 0 {
		return
	}

	zoom := math.Min(math.Max(v.zoom*factor, minZoom), maxZoom)
	if zoom == v.zoom {
		return
	}

	imageX := (float64(cursorX) - float64(current.x)) / current.scale
	imageY := (float64(cursorY) - float64(current.y)) / current.scale

	scale := current.scale / v.zoom * zoom
	v.zoom = zoom
	drawWidth := max(int(math.Round(float64(v.width)*scale)), 1)
	drawHeight := max(int(math.Round(float64(v.height)*scale)), 1)

	v.offsetX = float64(cursorX) - imageX*scale - float64((width-drawWidth)/2)
	v.offsetY = float64(cursorY) - imageY*scale - float64((height-drawHeight)/2)
	v.layout(width, height)

	procInvalidateRect.Call(v.hwnd, 0, 0)
}

func (v *viewer) pan(dx, dy int) {
	if dx == 0 && dy == 0 {
		return
	}
	v.offsetX += float64(dx)
	v.offsetY += float64(dy)

	var rect winRect
	if ok, _, _ := procGetClientRect.Call(v.hwnd, uintptr(unsafe.Pointer(&rect))); ok == 0 {
		return
	}
	v.layout(int(rect.right-rect.left), int(rect.bottom-rect.top))
	procInvalidateRect.Call(v.hwnd, 0, 0)
}

func (v *viewer) ensureBackBuffer(hdc uintptr, width, height int) bool {
	if v.memDC != 0 && v.memWidth == width && v.memHeight == height {
		return true
	}
	v.releaseBackBuffer()

	memDC, _, _ := procCreateCompatibleDC.Call(hdc)
	if memDC == 0 {
		return false
	}
	memBitmap, _, _ := procCreateCompatibleBitmap.Call(hdc, uintptr(width), uintptr(height))
	if memBitmap == 0 {
		procDeleteDC.Call(memDC)
		return false
	}
	old, _, _ := procSelectObject.Call(memDC, memBitmap)

	v.memDC = memDC
	v.memBitmap = memBitmap
	v.memOld = old
	v.memWidth = width
	v.memHeight = height
	return true
}

func (v *viewer) releaseBackBuffer() {
	if v.memDC == 0 {
		return
	}
	if v.memOld != 0 {
		procSelectObject.Call(v.memDC, v.memOld)
	}
	procDeleteObject.Call(v.memBitmap)
	procDeleteDC.Call(v.memDC)
	v.memDC = 0
	v.memBitmap = 0
	v.memOld = 0
	v.memWidth = 0
	v.memHeight = 0
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

	target := hdc
	if v.ensureBackBuffer(hdc, width, height) {
		target = v.memDC
	}

	// COLORREF = 0x00BBGGRR; #1c2028 → R=0x1c G=0x20 B=0x28 → 0x28201c
	hBrush, _, _ := procCreateSolidBrush.Call(0x28201c)
	defer procDeleteObject.Call(hBrush)

	procFillRect.Call(target, uintptr(unsafe.Pointer(&rect)), hBrush)

	g := v.layout(width, height)
	if len(v.pixels) > 0 && v.width > 0 && v.height > 0 && g.width > 0 && g.height > 0 {
		procSetStretchBltMode.Call(target, halftone)
		procSetBrushOrgEx.Call(target, 0, 0, 0)

		info := bitmapInfoHeader{
			size:      uint32(unsafe.Sizeof(bitmapInfoHeader{})),
			width:     int32(v.width),
			height:    -int32(v.height),
			planes:    1,
			bitCount:  32,
			sizeImage: uint32(len(v.pixels)),
		}

		procStretchDIBits.Call(
			target,
			uintptr(g.x), uintptr(g.y), uintptr(g.width), uintptr(g.height),
			0, 0, uintptr(v.width), uintptr(v.height),
			uintptr(unsafe.Pointer(&v.pixels[0])),
			uintptr(unsafe.Pointer(&info)),
			dibRGBColors,
			srcCopy,
		)
	}

	if target != hdc {
		procBitBlt.Call(
			hdc, 0, 0, uintptr(width), uintptr(height),
			target, 0, 0,
			srcCopy,
		)
	}
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
