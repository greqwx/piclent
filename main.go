// Copyright (c) 2026 Greg and Contributors

package main

import (
	"bytes"
	_ "embed"
	"fmt"
	"image"
	"image/color"
	"image/draw"
	_ "image/gif"
	_ "image/jpeg"
	_ "image/png"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"syscall"
	"unsafe"

	_ "golang.org/x/image/bmp"
)

//go:embed assets/icon-16.png
var icon16PNG []byte

//go:embed assets/icon-128.png
var icon128PNG []byte

type frame struct {
	pixels []byte
	width  int
	height int
}

// activeViewer keeps the viewer reachable for the process lifetime; the window
// stores it as an integer via GWLP_USERDATA, which the garbage collector does
// not scan
var activeViewer *viewer

func main() {
	runtime.LockOSThread()
	os.Exit(run())
}

func run() int {
	path, budgetMB, err := parseArgs(os.Args[1:])
	if err != nil {
		fatal("%v", err)
		return 2
	}
	path = filepath.Clean(path)

	config, err := readConfig(path)
	if err != nil {
		fatal("%v", err)
		return 1
	}

	g := newGallery(path, budgetMB*1024*1024)

	hInstance, _, _ := procGetModuleHandleW.Call(0)

	hIcon := loadIconFromPNG(icon128PNG, hInstance)
	hIconSm := loadIconFromPNG(icon16PNG, hInstance)

	className, _ := syscall.UTF16PtrFromString("piclent_wnd")

	wc := wndClassExW{
		cbSize:        uint32(unsafe.Sizeof(wndClassExW{})),
		style:         csHRedraw | csVRedraw,
		lpfnWndProc:   syscall.NewCallback(wndProc),
		hInstance:     hInstance,
		hIcon:         hIcon,
		hIconSm:       hIconSm,
		hCursor:       loadCursor(idcArrow),
		hbrBackground: colorWindow + 1,
		lpszClassName: className,
	}
	if atom, _, _ := procRegisterClassExW.Call(uintptr(unsafe.Pointer(&wc))); atom == 0 {
		fatal("RegisterClassEx failed")
		return 1
	}

	monX, monY, monW, monH := workArea()
	winW, winH := initialWindowSize(config.Width, config.Height, monW, monH)

	// center on monitor
	x := monX + (monW-winW)/2
	y := monY + (monH-winH)/2

	title, _ := syscall.UTF16PtrFromString(filepath.Base(path))

	style := uint32(wsOverlappedWindow)
	var r winRect
	r.right = int32(winW)
	r.bottom = int32(winH)
	procAdjustWindowRect.Call(uintptr(unsafe.Pointer(&r)), uintptr(style), 0)
	adjW := int(r.right - r.left)
	adjH := int(r.bottom - r.top)

	hwnd, _, _ := procCreateWindowExW.Call(
		0,
		uintptr(unsafe.Pointer(className)),
		uintptr(unsafe.Pointer(title)),
		uintptr(style),
		uintptr(x), uintptr(y), uintptr(adjW), uintptr(adjH),
		0, 0, hInstance, 0,
	)
	if hwnd == 0 {
		fatal("CreateWindow failed")
		return 1
	}

	// dark titlebar (Windows 10 20H1+ and Windows 11)
	darkMode := uint32(1)
	procDwmSetWindowAttribute.Call(hwnd, dwmwaUseImmersiveDarkMode, uintptr(unsafe.Pointer(&darkMode)), unsafe.Sizeof(darkMode))

	// store viewer pointer in window's user data
	v := &viewer{hwnd: hwnd, zoom: minZoom, gallery: g}
	activeViewer = v
	procSetWindowLongPtrW.Call(hwnd, gwlUserdata, uintptr(unsafe.Pointer(v)))

	procShowWindow.Call(hwnd, swShow)
	procUpdateWindow.Call(hwnd)

	maxW, maxH := monW, monH
	if maxW <= 0 {
		maxW = winW
	}
	if maxH <= 0 {
		maxH = winH
	}

	g.hwnd = hwnd
	g.start(maxW, maxH)

	var msg msgW
	for {
		r, _, _ := procGetMessageW.Call(uintptr(unsafe.Pointer(&msg)), 0, 0, 0)
		if r == 0 || r == ^uintptr(0) { // 0 = WM_QUIT, -1 = error
			break
		}
		procTranslateMessage.Call(uintptr(unsafe.Pointer(&msg)))
		procDispatchMessageW.Call(uintptr(unsafe.Pointer(&msg)))
	}

	return 0
}

func wndProc(hwnd, msg, wParam, lParam uintptr) uintptr {
	switch msg {
	case wmUserFrame:
		if v := getViewer(hwnd); v != nil {
			v.refresh()
		}
		return 0

	case wmPaint:
		v := getViewer(hwnd)
		if v != nil {
			v.paint()
		} else {
			// no viewer yet — just validate
			var ps paintStruct
			procBeginPaint.Call(hwnd, uintptr(unsafe.Pointer(&ps)))
			procEndPaint.Call(hwnd, uintptr(unsafe.Pointer(&ps)))
		}
		return 0

	case wmSize:
		v := getViewer(hwnd)
		if v != nil {
			procInvalidateRect.Call(hwnd, 0, 0)
		}
		return 0

	case wmEraseBkgnd:
		return 1 // prevent flicker — WM_PAINT handles background

	case wmMouseWheel:
		v := getViewer(hwnd)
		if v != nil {
			x, y := mousePoint(lParam)
			v.zoomWheel(x, y, int(int16(uint16(wParam>>16))))
		}
		return 0

	case wmKeyDown:
		v := getViewer(hwnd)
		if v != nil {
			switch int(uint16(wParam)) {
			case vkLeft, vkPrior:
				v.navigate(-1)
				return 0
			case vkRight, vkNext:
				v.navigate(1)
				return 0
			}
		}

	case wmSetCursor:
		if int(lParam&0xffff) == htClient {
			if v := getViewer(hwnd); v != nil {
				if v.dragging {
					procSetCursor.Call(loadCursor(idcSizeAll))
					return 1
				}
				var pt point
				procGetCursorPos.Call(uintptr(unsafe.Pointer(&pt)))
				procScreenToClient.Call(hwnd, uintptr(unsafe.Pointer(&pt)))
				if v.buttonAt(int(pt.x), int(pt.y)) != 0 {
					procSetCursor.Call(loadCursor(idcHand))
					return 1
				}
			}
		}

	case wmLButtonDown:
		v := getViewer(hwnd)
		if v != nil {
			x, y := mousePoint(lParam)
			if direction := v.buttonAt(x, y); direction != 0 {
				v.navigate(direction)
				return 0
			}
			if !v.dragging {
				v.dragging = true
				v.dragX, v.dragY = x, y
				procSetCapture.Call(hwnd)
				procSetCursor.Call(loadCursor(idcSizeAll))
			}
		}
		return 0

	case wmRButtonDown:
		v := getViewer(hwnd)
		if v != nil && !v.dragging {
			v.dragging = true
			v.dragX, v.dragY = mousePoint(lParam)
			procSetCapture.Call(hwnd)
			procSetCursor.Call(loadCursor(idcSizeAll))
		}
		return 0

	case wmMouseMove:
		v := getViewer(hwnd)
		if v != nil && v.dragging {
			x, y := mousePoint(lParam)
			v.pan(x-v.dragX, y-v.dragY)
			v.dragX, v.dragY = x, y
			procSetCursor.Call(loadCursor(idcSizeAll))
		}
		return 0

	case wmLButtonUp, wmRButtonUp:
		v := getViewer(hwnd)
		if v != nil && v.dragging {
			v.dragging = false
			procReleaseCapture.Call()
			procSetCursor.Call(loadCursor(idcArrow))
		}
		return 0

	case wmCaptureChanged:
		v := getViewer(hwnd)
		if v != nil {
			v.dragging = false
		}
		return 0

	case wmDestroy:
		if v := getViewer(hwnd); v != nil {
			v.releaseBackBuffer()
			v.releaseNavGlyphs()
		}
		procPostQuitMessage.Call(0)
		return 0
	}
	r, _, _ := procDefWindowProcW.Call(hwnd, msg, wParam, lParam)
	return r
}

func parseArgs(args []string) (string, int, error) {
	const usage = "Drag & drop an image onto piclent.exe to open it."
	budget := 256
	path := ""

	for i := 0; i < len(args); i++ {
		arg := args[i]
		switch {
		case arg == "-m" || arg == "--m":
			if i+1 >= len(args) {
				return "", 0, fmt.Errorf("%s", usage)
			}
			i++
			value, err := strconv.Atoi(args[i])
			if err != nil || value <= 0 {
				return "", 0, fmt.Errorf("invalid memory limit: %s", args[i])
			}
			budget = value
		case strings.HasPrefix(arg, "-m=") || strings.HasPrefix(arg, "--m="):
			value, err := strconv.Atoi(arg[strings.Index(arg, "=")+1:])
			if err != nil || value <= 0 {
				return "", 0, fmt.Errorf("invalid memory limit: %s", arg)
			}
			budget = value
		case strings.HasPrefix(arg, "-"):
			return "", 0, fmt.Errorf("%s", usage)
		default:
			if path != "" {
				return "", 0, fmt.Errorf("%s", usage)
			}
			path = arg
		}
	}

	if path == "" {
		return "", 0, fmt.Errorf("%s", usage)
	}
	return path, budget, nil
}

func mousePoint(lParam uintptr) (int, int) {
	return int(int16(uint16(lParam))), int(int16(uint16(lParam >> 16)))
}

func getViewer(hwnd uintptr) *viewer {
	ptr, _, _ := procGetWindowLongPtrW.Call(hwnd, gwlUserdata)
	if ptr == 0 {
		return nil
	}
	return (*viewer)(unsafe.Pointer(ptr))
}

func readConfig(path string) (image.Config, error) {
	file, err := os.Open(path)
	if err != nil {
		return image.Config{}, err
	}
	defer file.Close()

	config, _, err := image.DecodeConfig(file)
	if err != nil {
		return image.Config{}, fmt.Errorf("%s: %w", path, err)
	}
	return config, nil
}

func loadFrame(path string, maxWidth, maxHeight int) (*frame, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	img, _, err := image.Decode(file)
	if err != nil {
		return nil, fmt.Errorf("%s: %w", path, err)
	}

	bounds := img.Bounds()
	rgba := image.NewRGBA(image.Rect(0, 0, bounds.Dx(), bounds.Dy()))
	draw.Draw(rgba, rgba.Bounds(), image.NewUniform(color.RGBA{R: 0x1c, G: 0x20, B: 0x28, A: 0xff}), image.Point{}, draw.Src)
	draw.Draw(rgba, rgba.Bounds(), img, bounds.Min, draw.Over)

	width, height := rgba.Bounds().Dx(), rgba.Bounds().Dy()
	if maxWidth > 0 && maxHeight > 0 && (width > maxWidth || height > maxHeight) {
		scale := min(float64(maxWidth)/float64(width), float64(maxHeight)/float64(height))
		scaledWidth := max(int(float64(width)*scale+0.5), 1)
		scaledHeight := max(int(float64(height)*scale+0.5), 1)
		rgba = downscale(rgba, scaledWidth, scaledHeight)
		width, height = scaledWidth, scaledHeight
	}

	return &frame{
		pixels: toBGRA(rgba),
		width:  width,
		height: height,
	}, nil
}

func toBGRA(img *image.RGBA) []byte {
	pixels := make([]byte, len(img.Pix))
	for i := 0; i < len(img.Pix); i += 4 {
		pixels[i] = img.Pix[i+2]
		pixels[i+1] = img.Pix[i+1]
		pixels[i+2] = img.Pix[i]
		pixels[i+3] = 0xff
	}
	return pixels
}

func downscale(src *image.RGBA, width, height int) *image.RGBA {
	dst := image.NewRGBA(image.Rect(0, 0, width, height))
	srcWidth := src.Bounds().Dx()
	srcHeight := src.Bounds().Dy()

	for y := 0; y < height; y++ {
		y0 := y * srcHeight / height
		y1 := (y + 1) * srcHeight / height
		if y1 <= y0 {
			y1 = y0 + 1
		}
		for x := 0; x < width; x++ {
			x0 := x * srcWidth / width
			x1 := (x + 1) * srcWidth / width
			if x1 <= x0 {
				x1 = x0 + 1
			}
			var r, g, b, a uint64
			for sy := y0; sy < y1; sy++ {
				row := src.Pix[sy*src.Stride:]
				for sx := x0; sx < x1; sx++ {
					i := sx * 4
					r += uint64(row[i])
					g += uint64(row[i+1])
					b += uint64(row[i+2])
					a += uint64(row[i+3])
				}
			}
			count := uint64((y1 - y0) * (x1 - x0))
			i := y*dst.Stride + x*4
			dst.Pix[i] = uint8((r + count/2) / count)
			dst.Pix[i+1] = uint8((g + count/2) / count)
			dst.Pix[i+2] = uint8((b + count/2) / count)
			dst.Pix[i+3] = uint8((a + count/2) / count)
		}
	}
	return dst
}

func initialWindowSize(imageWidth, imageHeight, areaWidth, areaHeight int) (int, int) {
	width, height := imageWidth, imageHeight

	if areaWidth > 0 {
		maxWidth := max(areaWidth*9/10-2*imageInsetX, 1)
		if width > maxWidth {
			height = height * maxWidth / width
			width = maxWidth
		}
	}
	if areaHeight > 0 {
		maxHeight := max(areaHeight*9/10-2*imageInsetY, 1)
		if height > maxHeight {
			width = width * maxHeight / height
			height = maxHeight
		}
	}
	if width < 1 {
		width = 1
	}
	if height < 1 {
		height = 1
	}
	return width + 2*imageInsetX, height + 2*imageInsetY
}

func workArea() (x, y, width, height int) {
	var info monitorInfo
	info.cbSize = uint32(unsafe.Sizeof(info))
	// get primary monitor via point (0,0)
	hMon, _, _ := procMonitorFromPoint.Call(0, 0, monitorDefaultToPrimary)
	if hMon == 0 {
		return
	}
	if ok, _, _ := procGetMonitorInfoW.Call(hMon, uintptr(unsafe.Pointer(&info))); ok == 0 {
		return
	}
	x = int(info.rcWork.left)
	y = int(info.rcWork.top)
	width = int(info.rcWork.right - info.rcWork.left)
	height = int(info.rcWork.bottom - info.rcWork.top)
	return
}

func loadIconFromPNG(data []byte, hInstance uintptr) uintptr {
	img, _, err := image.Decode(bytes.NewReader(data))
	if err != nil {
		return 0
	}
	b := img.Bounds()
	w, h := b.Dx(), b.Dy()

	// convert to BGRA top-down
	rgba, ok := img.(*image.RGBA)
	if !ok {
		tmp := image.NewRGBA(b)
		draw.Draw(tmp, tmp.Bounds(), img, b.Min, draw.Over)
		rgba = tmp
	}
	bgra := make([]byte, w*h*4)
	for i := 0; i < len(rgba.Pix); i += 4 {
		bgra[i] = rgba.Pix[i+2]
		bgra[i+1] = rgba.Pix[i+1]
		bgra[i+2] = rgba.Pix[i]
		bgra[i+3] = rgba.Pix[i+3]
	}

	hbmp, _, _ := procCreateBitmap.Call(
		uintptr(w), uintptr(h), 1, 32,
		uintptr(unsafe.Pointer(&bgra[0])),
	)
	if hbmp == 0 {
		return 0
	}
	defer procDeleteObject.Call(hbmp)

	// build 1-bit mask (all transparent)
	maskSize := ((w + 15) / 16) * 2 * h
	mask := make([]byte, maskSize)
	hMask, _, _ := procCreateBitmap.Call(
		uintptr(w), uintptr(h), 1, 1,
		uintptr(unsafe.Pointer(&mask[0])),
	)
	if hMask == 0 {
		return 0
	}
	defer procDeleteObject.Call(hMask)

	ii := iconInfo{
		fIcon:    1,
		hbmMask:  hMask,
		hbmColor: hbmp,
	}
	hIcon, _, _ := procCreateIconIndirect.Call(uintptr(unsafe.Pointer(&ii)))
	return hIcon
}

func loadCursor(resID uintptr) uintptr {
	h, _, _ := procLoadCursorW.Call(0, resID)
	return h
}
