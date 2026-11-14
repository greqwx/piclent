// Copyright (c) 2026 Greg and Contributors

package main

import (
	"math"
	"syscall"
	"time"
	"unsafe"
)

const (
	minZoom         = 1.0
	maxZoom         = 8.0
	wheelZoomFactor = 1.2

	// nav button geometry and opacity
	navButtonSize   = 30
	navButtonMargin = 12
	navButtonAlpha  = 0.75
	navArrowAlpha   = 0.92

	// dark borders around the image so it never sits under the nav buttons
	imageInsetX = 56
	imageInsetY = 28
)

type viewer struct {
	hwnd    uintptr
	gallery *gallery
	pixels  []byte
	width   int
	height  int
	message string

	anim      *animation
	animFrame int
	animGen   int
	animStop  chan struct{}
	paused    bool
	suspended bool

	zoom    float64
	offsetX float64
	offsetY float64

	dragging bool
	dragX    int
	dragY    int

	fullscreen     bool
	savedStyle     uintptr
	savedPlacement windowPlacement

	memDC     uintptr
	memBitmap uintptr
	memOld    uintptr
	memWidth  int
	memHeight int

	navPrev *navGlyph
	navNext *navGlyph
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
	insetX, insetY := imageInsets(clientWidth, clientHeight)
	innerWidth := clientWidth - 2*insetX
	innerHeight := clientHeight - 2*insetY

	fitWidth, fitHeight := fitSize(v.width, v.height, innerWidth, innerHeight)
	if fitWidth <= 0 || fitHeight <= 0 {
		return viewGeometry{}
	}

	scale := float64(fitWidth) / float64(v.width) * v.zoom
	drawWidth := max(int(math.Round(float64(v.width)*scale)), 1)
	drawHeight := max(int(math.Round(float64(v.height)*scale)), 1)

	v.offsetX = clampOffset(v.offsetX, drawWidth, clientWidth)
	v.offsetY = clampOffset(v.offsetY, drawHeight, clientHeight)

	return viewGeometry{
		x:      insetX + (innerWidth-drawWidth)/2 + int(math.Round(v.offsetX)),
		y:      insetY + (innerHeight-drawHeight)/2 + int(math.Round(v.offsetY)),
		width:  drawWidth,
		height: drawHeight,
		scale:  scale,
	}
}

// imageInsets keeps dark borders at the window edges where the buttons live
func imageInsets(width, height int) (int, int) {
	insetX, insetY := imageInsetX, imageInsetY
	if width-2*insetX < 1 {
		insetX = 0
	}
	if height-2*insetY < 1 {
		insetY = 0
	}
	return insetX, insetY
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

func (v *viewer) hasNavigation() bool {
	return v.gallery != nil && v.gallery.count() > 1
}

// navigate switches to the neighboring image and resets the view
func (v *viewer) navigate(direction int) {
	if !v.hasNavigation() {
		return
	}
	if !v.gallery.step(direction) {
		return
	}
	v.zoom = minZoom
	v.offsetX = 0
	v.offsetY = 0
	v.message = ""
	v.paused = false
	v.updateTitle()
	v.refresh()
}

func (v *viewer) updateTitle() {
	if v.gallery == nil {
		return
	}
	title, _ := syscall.UTF16PtrFromString(v.gallery.currentName())
	procSetWindowTextW.Call(v.hwnd, uintptr(unsafe.Pointer(title)))
}

// refresh pulls the current frame from the cache or switches to its error
func (v *viewer) refresh() {
	if v.gallery == nil {
		return
	}
	f, message := v.gallery.snapshot()
	switch {
	case message != "":
		v.message = message
		v.pixels = nil
		v.width = 0
		v.height = 0
		v.anim = nil
		v.animFrame = 0
		v.updatePlayback()
	case f != nil:
		v.message = ""
		v.width = f.width
		v.height = f.height
		v.anim = f.anim
		v.animFrame = 0
		if f.anim != nil {
			v.pixels = f.anim.frames[0]
		} else {
			v.pixels = f.pixels
		}
		v.updatePlayback()
	default:
		// the next image is still decoding, keep the previous one frozen
		v.stopPlayback()
	}
	procInvalidateRect.Call(v.hwnd, 0, 0)
}

// updatePlayback starts the frame ticker only when there is something to play
func (v *viewer) updatePlayback() {
	v.stopPlayback()
	if v.anim == nil || len(v.anim.frames) < 2 || v.paused || v.suspended {
		return
	}

	gen := v.animGen
	stop := make(chan struct{})
	v.animStop = stop

	anim := v.anim
	hwnd := v.hwnd
	index := v.animFrame

	beginTimerResolution()
	go func() {
		defer endTimerResolution()

		plays := 1
		nextAt := time.Now().Add(frameDuration(anim.delays[index]))
		for {
			if wait := time.Until(nextAt); wait > 0 {
				timer := time.NewTimer(wait)
				select {
				case <-stop:
					timer.Stop()
					return
				case <-timer.C:
				}
				continue
			}

			// the machine fell behind, jump to the frame that should be shown now
			advanced := false
			for {
				next := index + 1
				if next >= len(anim.frames) {
					plays++
					if anim.loops > 0 && plays > anim.loops {
						return
					}
					next = 0
				}
				index = next
				advanced = true
				nextAt = nextAt.Add(frameDuration(anim.delays[index]))
				if time.Now().Before(nextAt) {
					break
				}
			}
			if advanced {
				procPostMessageW.Call(hwnd, wmUserAnim, uintptr(gen), uintptr(index))
			}
		}
	}()
}

func (v *viewer) stopPlayback() {
	// bumping the generation drops frames already queued in the message loop
	v.animGen++
	if v.animStop == nil {
		return
	}
	close(v.animStop)
	v.animStop = nil
}

func (v *viewer) showAnimationFrame(gen, index int) {
	if v.anim == nil || gen != v.animGen || index < 0 || index >= len(v.anim.frames) {
		return
	}
	v.animFrame = index
	v.pixels = v.anim.frames[index]
	procInvalidateRect.Call(v.hwnd, 0, 0)
}

func (v *viewer) togglePause() {
	if v.anim == nil || len(v.anim.frames) < 2 {
		return
	}
	v.paused = !v.paused
	v.updatePlayback()
}

func (v *viewer) setSuspended(suspended bool) {
	if v.suspended == suspended {
		return
	}
	v.suspended = suspended
	v.updatePlayback()
}

func (v *viewer) toggleFullscreen() {
	if v.fullscreen {
		v.exitFullscreen()
	} else {
		v.enterFullscreen()
	}
}

// enterFullscreen covers the current monitor with a borderless topmost window
func (v *viewer) enterFullscreen() {
	saved := windowPlacement{length: uint32(unsafe.Sizeof(windowPlacement{}))}
	if ok, _, _ := procGetWindowPlacement.Call(v.hwnd, uintptr(unsafe.Pointer(&saved))); ok == 0 {
		return
	}
	hMon, _, _ := procMonitorFromWindow.Call(v.hwnd, monitorDefaultToNearest)
	if hMon == 0 {
		return
	}
	var info monitorInfo
	info.cbSize = uint32(unsafe.Sizeof(info))
	if ok, _, _ := procGetMonitorInfoW.Call(hMon, uintptr(unsafe.Pointer(&info))); ok == 0 {
		return
	}

	style, _, _ := procGetWindowLongPtrW.Call(v.hwnd, gwlStyle)
	v.savedStyle = style
	v.savedPlacement = saved
	v.fullscreen = true

	procSetWindowLongPtrW.Call(v.hwnd, gwlStyle, wsPopup|wsVisible)
	procSetWindowPos.Call(
		v.hwnd, hwndTopmost,
		uintptr(info.rcMonitor.left), uintptr(info.rcMonitor.top),
		uintptr(info.rcMonitor.right-info.rcMonitor.left),
		uintptr(info.rcMonitor.bottom-info.rcMonitor.top),
		swpFrameChanged|swpShowWindow,
	)
}

func (v *viewer) exitFullscreen() {
	if !v.fullscreen {
		return
	}
	v.fullscreen = false

	procSetWindowLongPtrW.Call(v.hwnd, gwlStyle, v.savedStyle)
	procSetWindowPlacement.Call(v.hwnd, uintptr(unsafe.Pointer(&v.savedPlacement)))
	procSetWindowPos.Call(
		v.hwnd, hwndNotTopmost,
		0, 0, 0, 0,
		swpNoMove|swpNoSize|swpNoZOrder|swpFrameChanged|swpShowWindow,
	)
	procInvalidateRect.Call(v.hwnd, 0, 0)
}

func frameDuration(ms int) time.Duration {
	if ms <= 0 {
		ms = 20
	}
	return time.Duration(ms) * time.Millisecond
}

// gif delays are short enough that the default 15.6 ms timer tick is visible
func beginTimerResolution() {
	procTimeBeginPeriod.Call(1)
}

func endTimerResolution() {
	procTimeEndPeriod.Call(1)
}

func navButtonRects(width, height int) (winRect, winRect) {
	size := navButtonSize
	if height < size*3 {
		size = max(height/3, size/2)
	}
	margin := navButtonMargin
	top := (height - size) / 2
	prev := winRect{
		left:   int32(margin),
		top:    int32(top),
		right:  int32(margin + size),
		bottom: int32(top + size),
	}
	next := winRect{
		left:   int32(width - margin - size),
		top:    int32(top),
		right:  int32(width - margin),
		bottom: int32(top + size),
	}
	return prev, next
}

func pointInRect(r winRect, x, y int) bool {
	return int32(x) >= r.left && int32(x) < r.right && int32(y) >= r.top && int32(y) < r.bottom
}

// buttonAt returns -1 for the previous button, 1 for the next one, 0 for none
func (v *viewer) buttonAt(x, y int) int {
	if !v.hasNavigation() {
		return 0
	}
	var rect winRect
	if ok, _, _ := procGetClientRect.Call(v.hwnd, uintptr(unsafe.Pointer(&rect))); ok == 0 {
		return 0
	}
	width := int(rect.right - rect.left)
	height := int(rect.bottom - rect.top)
	prev, next := navButtonRects(width, height)
	if pointInRect(prev, x, y) {
		return -1
	}
	if pointInRect(next, x, y) {
		return 1
	}
	return 0
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

	if v.message != "" {
		drawMessage(target, width, height, v.message)
	} else if len(v.pixels) > 0 && v.width > 0 && v.height > 0 {
		g := v.layout(width, height)
		if g.width > 0 && g.height > 0 {
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
	}

	if v.hasNavigation() {
		prev, next := navButtonRects(width, height)
		v.drawNavButton(target, prev, -1)
		v.drawNavButton(target, next, 1)
	}

	if target != hdc {
		procBitBlt.Call(
			hdc, 0, 0, uintptr(width), uintptr(height),
			target, 0, 0,
			srcCopy,
		)
	}
}

func drawMessage(hdc uintptr, width, height int, message string) {
	font, _, _ := procGetStockObject.Call(defaultGuiFont)
	oldFont, _, _ := procSelectObject.Call(hdc, font)
	defer procSelectObject.Call(hdc, oldFont)

	procSetBkMode.Call(hdc, bkTransparent)
	procSetTextColor.Call(hdc, messageColor)

	text, _ := syscall.UTF16PtrFromString(message)
	rect := winRect{
		left:   40,
		top:    40,
		right:  int32(width - 40),
		bottom: int32(height - 40),
	}

	measured := rect
	procDrawTextW.Call(
		hdc,
		uintptr(unsafe.Pointer(text)),
		^uintptr(0),
		uintptr(unsafe.Pointer(&measured)),
		dtCalcRect|dtCenter|dtWordBreak,
	)
	textHeight := int(measured.bottom - measured.top)
	top := (height - textHeight) / 2
	rect.top = int32(top)
	rect.bottom = int32(top + textHeight)

	procDrawTextW.Call(
		hdc,
		uintptr(unsafe.Pointer(text)),
		^uintptr(0),
		uintptr(unsafe.Pointer(&rect)),
		dtCenter|dtWordBreak,
	)
}

// navGlyph is an antialiased button image with per-pixel alpha
type navGlyph struct {
	dc     uintptr
	bitmap uintptr
	old    uintptr
	size   int
}

func (v *viewer) navGlyph(size, direction int) *navGlyph {
	if direction < 0 {
		if v.navPrev == nil || v.navPrev.size != size {
			releaseNavGlyph(&v.navPrev)
			v.navPrev = newNavGlyph(size, -1)
		}
		return v.navPrev
	}
	if v.navNext == nil || v.navNext.size != size {
		releaseNavGlyph(&v.navNext)
		v.navNext = newNavGlyph(size, 1)
	}
	return v.navNext
}

func (v *viewer) releaseNavGlyphs() {
	releaseNavGlyph(&v.navPrev)
	releaseNavGlyph(&v.navNext)
}

func releaseNavGlyph(g **navGlyph) {
	if *g == nil {
		return
	}
	if (*g).old != 0 {
		procSelectObject.Call((*g).dc, (*g).old)
	}
	procDeleteObject.Call((*g).bitmap)
	procDeleteDC.Call((*g).dc)
	*g = nil
}

func newNavGlyph(size, direction int) *navGlyph {
	if size <= 0 {
		return nil
	}
	screen, _, _ := procGetDC.Call(0)
	if screen == 0 {
		return nil
	}
	defer procReleaseDC.Call(0, screen)

	dc, _, _ := procCreateCompatibleDC.Call(screen)
	if dc == 0 {
		return nil
	}

	header := bitmapInfoHeader{
		size:      uint32(unsafe.Sizeof(bitmapInfoHeader{})),
		width:     int32(size),
		height:    -int32(size),
		planes:    1,
		bitCount:  32,
		sizeImage: uint32(size * size * 4),
	}
	var bits uintptr
	bitmap, _, _ := procCreateDIBSection.Call(
		dc,
		uintptr(unsafe.Pointer(&header)),
		dibRGBColors,
		uintptr(unsafe.Pointer(&bits)),
		0,
		0,
	)
	if bitmap == 0 {
		procDeleteDC.Call(dc)
		return nil
	}
	old, _, _ := procSelectObject.Call(dc, bitmap)

	drawNavGlyph(bits, size, direction)
	return &navGlyph{dc: dc, bitmap: bitmap, old: old, size: size}
}

func (v *viewer) drawNavButton(hdc uintptr, rect winRect, direction int) {
	size := int(rect.right - rect.left)
	glyph := v.navGlyph(size, direction)
	if glyph == nil {
		return
	}
	// BLENDFUNCTION is a packed 4-byte struct passed by value
	blend := uintptr(255)<<16 | uintptr(acSrcAlpha)<<24
	procAlphaBlend.Call(
		hdc,
		uintptr(rect.left), uintptr(rect.top),
		uintptr(size), uintptr(rect.bottom-rect.top),
		glyph.dc, 0, 0,
		uintptr(size), uintptr(glyph.size),
		blend,
	)
}

// drawNavGlyph paints a translucent circle with a chevron into a top-down
// 32-bit DIB, premultiplying alpha so AlphaBlend can composite it
func drawNavGlyph(bits uintptr, size, direction int) {
	pixels := unsafe.Slice((*byte)(unsafe.Pointer(bits)), size*size*4)

	center := float64(size) / 2
	radius := center - 0.35
	width := float64(size) * 0.17
	arm := float64(size) * 0.24
	stroke := float64(size) * 0.05

	vertexX, armX := center+width, center-width
	if direction < 0 {
		vertexX, armX = center-width, center+width
	}
	// nudge the chevron toward the side it points to; a bare vertex reads as
	// off-centre otherwise
	shift := float64(size) * 0.05 * float64(direction)
	firstX, firstY := armX+shift, center-arm
	cornerX, cornerY := vertexX+shift, center
	lastX, lastY := armX+shift, center+arm

	buttonR, buttonG, buttonB := colorParts(uintptr(navButtonColor))
	arrowR, arrowG, arrowB := colorParts(uintptr(navArrowColor))

	for y := 0; y < size; y++ {
		for x := 0; x < size; x++ {
			cx := float64(x) + 0.5
			cy := float64(y) + 0.5

			circle := coverage(radius - math.Hypot(cx-center, cy-center))
			alpha := circle * navButtonAlpha
			r := buttonR * alpha
			g := buttonG * alpha
			b := buttonB * alpha

			chevron := 0.0
			if circle > 0 {
				distance := math.Min(
					segmentDistance(cx, cy, firstX, firstY, cornerX, cornerY),
					segmentDistance(cx, cy, cornerX, cornerY, lastX, lastY),
				)
				chevron = coverage(stroke-distance) * circle
			}
			if chevron > 0 {
				source := chevron * navArrowAlpha
				r = arrowR*source + r*(1-source)
				g = arrowG*source + g*(1-source)
				b = arrowB*source + b*(1-source)
				alpha = source + alpha*(1-source)
			}

			offset := (y*size + x) * 4
			pixels[offset] = byte(math.Round(b * 255))
			pixels[offset+1] = byte(math.Round(g * 255))
			pixels[offset+2] = byte(math.Round(r * 255))
			pixels[offset+3] = byte(math.Round(alpha * 255))
		}
	}
}

// coverage turns a signed distance to a shape edge into a 0..1 pixel share
func coverage(distance float64) float64 {
	switch {
	case distance >= 0.5:
		return 1
	case distance <= -0.5:
		return 0
	default:
		return distance + 0.5
	}
}

func segmentDistance(px, py, ax, ay, bx, by float64) float64 {
	dx, dy := bx-ax, by-ay
	length := dx*dx + dy*dy
	t := 0.0
	if length > 0 {
		t = ((px-ax)*dx + (py-ay)*dy) / length
		t = math.Min(math.Max(t, 0), 1)
	}
	return math.Hypot(px-(ax+t*dx), py-(ay+t*dy))
}

// colorParts splits a COLORREF (0x00BBGGRR) into normalized components
func colorParts(colorref uintptr) (r, g, b float64) {
	return float64(colorref&0xFF) / 255,
		float64((colorref>>8)&0xFF) / 255,
		float64((colorref>>16)&0xFF) / 255
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
