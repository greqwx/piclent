// Copyright (c) 2026 Greg and Contributors

package main

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"syscall"
	"unsafe"
)

const prefetchRadius = 2

// gallery keeps the folder listing and a byte-bounded LRU cache of frames
type gallery struct {
	mu     sync.Mutex
	hwnd   uintptr
	paths  []string
	index  int
	cache  map[string]*frame
	order  []string
	bytes  int
	budget int
	errors map[string]string
	busy   string
	wake   chan struct{}
}

func newGallery(path string, budget int) *gallery {
	paths, index := folderListing(path)

	return &gallery{
		paths:  paths,
		index:  index,
		cache:  make(map[string]*frame),
		budget: budget,
		errors: make(map[string]string),
		wake:   make(chan struct{}, 1),
	}
}

// folderListing builds the sorted folder contents and the index of path
func folderListing(path string) ([]string, int) {
	paths, err := listImages(filepath.Dir(path))
	if err != nil || len(paths) == 0 {
		paths = []string{path}
	}

	index := indexOfPath(paths, path)
	if index < 0 {
		paths = append(paths, path)
		sortPaths(paths)
		index = indexOfPath(paths, path)
	}
	return paths, index
}

func indexOfPath(paths []string, path string) int {
	for i, p := range paths {
		if strings.EqualFold(p, path) {
			return i
		}
	}
	return -1
}

func isImageFile(name string) bool {
	switch strings.ToLower(filepath.Ext(name)) {
	case ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".webp":
		return true
	}
	return false
}

func sortPaths(paths []string) {
	sort.Slice(paths, func(i, j int) bool {
		return naturalLess(filepath.Base(paths[i]), filepath.Base(paths[j]))
	})
}

// naturalLess compares names so that digit runs count as numbers, keeping
// photo-2 before photo-10
func naturalLess(a, b string) bool {
	a = strings.ToLower(a)
	b = strings.ToLower(b)

	ai, bi := 0, 0
	for ai < len(a) && bi < len(b) {
		if isDigit(a[ai]) && isDigit(b[bi]) {
			aEnd, bEnd := digitEnd(a, ai), digitEnd(b, bi)
			aNumber := strings.TrimLeft(a[ai:aEnd], "0")
			bNumber := strings.TrimLeft(b[bi:bEnd], "0")
			if len(aNumber) != len(bNumber) {
				return len(aNumber) < len(bNumber)
			}
			if aNumber != bNumber {
				return aNumber < bNumber
			}
			if aEnd-ai != bEnd-bi {
				return aEnd-ai < bEnd-bi
			}
			ai, bi = aEnd, bEnd
			continue
		}

		if a[ai] != b[bi] {
			return a[ai] < b[bi]
		}
		ai++
		bi++
	}
	return len(a)-ai < len(b)-bi
}

func isDigit(c byte) bool {
	return c >= '0' && c <= '9'
}

func digitEnd(s string, start int) int {
	i := start
	for i < len(s) && isDigit(s[i]) {
		i++
	}
	return i
}

func listImages(dir string) ([]string, error) {
	entries, err := os.ReadDir(dir)
	if err != nil {
		return nil, err
	}
	paths := make([]string, 0, len(entries))
	for _, entry := range entries {
		if entry.IsDir() || !isImageFile(entry.Name()) {
			continue
		}
		paths = append(paths, filepath.Join(dir, entry.Name()))
	}
	sortPaths(paths)
	return paths, nil
}

// start decodes images in the background, nearest to the current one first
func (g *gallery) start(maxWidth, maxHeight int) {
	animCap := g.budget / 2
	go func() {
		for {
			g.mu.Lock()
			path, full, ok := g.nextTaskLocked()
			g.mu.Unlock()
			if !ok {
				<-g.wake
				continue
			}

			f, err := loadFrame(path, maxWidth, maxHeight, animCap, full)

			g.mu.Lock()
			g.busy = ""
			if !g.containsLocked(path) {
				g.mu.Unlock()
				continue
			}
			current := g.paths[g.index] == path
			if err != nil {
				if old := g.cache[path]; old != nil && old.partial {
					// the first frame is still good, only the animation failed
					old.partial = false
				} else {
					g.errors[path] = err.Error()
				}
			} else {
				g.storeLocked(path, f)
			}
			g.mu.Unlock()

			if current {
				procPostMessageW.Call(g.hwnd, wmUserFrame, 0, 0)
			}
		}
	}()
	g.notify()
}

func (g *gallery) notify() {
	select {
	case g.wake <- struct{}{}:
	default:
	}
}

// nextTaskLocked picks the next file to decode; full tells whether it is the
// current one, whose whole animation is worth decoding
func (g *gallery) nextTaskLocked() (path string, full bool, ok bool) {
	if g.busy != "" {
		return "", false, false
	}
	indexes := []int{g.index}
	for radius := 1; radius <= prefetchRadius; radius++ {
		indexes = append(indexes, g.index+radius, g.index-radius)
	}
	for _, i := range indexes {
		if i < 0 || i >= len(g.paths) {
			continue
		}
		path = g.paths[i]
		current := i == g.index
		if cached := g.cache[path]; cached != nil {
			if !current || !cached.partial {
				continue
			}
		} else if _, failed := g.errors[path]; failed {
			continue
		}
		g.busy = path
		return path, current, true
	}
	return "", false, false
}

// step moves through the folder and reports whether the index changed
func (g *gallery) step(delta int) bool {
	g.mu.Lock()
	next := g.index + delta
	changed := next >= 0 && next < len(g.paths) && next != g.index
	if changed {
		g.index = next
	}
	g.mu.Unlock()

	if changed {
		g.notify()
	}
	return changed
}

func (g *gallery) snapshot() (*frame, string) {
	g.mu.Lock()
	defer g.mu.Unlock()

	if len(g.paths) == 0 {
		return nil, ""
	}
	path := g.paths[g.index]
	if f := g.cache[path]; f != nil {
		g.touchLocked(path)
		return f, ""
	}
	return nil, g.errors[path]
}

func (g *gallery) count() int {
	g.mu.Lock()
	defer g.mu.Unlock()
	return len(g.paths)
}

func (g *gallery) currentName() string {
	g.mu.Lock()
	defer g.mu.Unlock()
	return filepath.Base(g.paths[g.index])
}

func (g *gallery) currentPath() string {
	g.mu.Lock()
	defer g.mu.Unlock()
	if len(g.paths) == 0 {
		return ""
	}
	return g.paths[g.index]
}

// recycleFile sends the file to the Recycle Bin
func recycleFile(path string) error {
	// SHFileOperation needs a fully qualified path
	absolute, err := filepath.Abs(path)
	if err != nil {
		return err
	}
	from, err := syscall.UTF16FromString(absolute)
	if err != nil {
		return err
	}
	// the path list is terminated by an empty string
	from = append(from, 0)

	op := shFileOpStructW{
		wFunc:  foDelete,
		pFrom:  &from[0],
		fFlags: fofSilent | fofNoConfirmation | fofAllowUndo | fofNoErrorUI,
	}
	if result, _, _ := procSHFileOperationW.Call(uintptr(unsafe.Pointer(&op))); result != 0 {
		return fmt.Errorf("cannot move %s to the Recycle Bin", filepath.Base(path))
	}
	if op.fAnyOperationsAborted != 0 {
		return fmt.Errorf("%s was not deleted", filepath.Base(path))
	}
	return nil
}

func (g *gallery) containsLocked(path string) bool {
	for _, p := range g.paths {
		if p == path {
			return true
		}
	}
	return false
}

// reset points the gallery at the folder of path and drops the old cache
func (g *gallery) reset(path string) {
	paths, index := folderListing(path)

	g.mu.Lock()
	g.paths = paths
	g.index = index
	g.cache = make(map[string]*frame)
	g.order = nil
	g.bytes = 0
	g.errors = make(map[string]string)
	g.busy = ""
	g.mu.Unlock()

	g.notify()
}

// forgetCurrent drops the current entry after its file left the disk and
// returns how many files are left
func (g *gallery) forgetCurrent() int {
	g.mu.Lock()
	path := g.paths[g.index]
	g.paths = append(g.paths[:g.index], g.paths[g.index+1:]...)
	if old := g.cache[path]; old != nil {
		g.bytes -= old.size()
		g.removeLocked(path)
	}
	delete(g.errors, path)
	if g.index >= len(g.paths) {
		g.index = len(g.paths) - 1
	}
	remaining := len(g.paths)
	g.mu.Unlock()

	g.notify()
	return remaining
}

func (g *gallery) storeLocked(path string, f *frame) {
	if old := g.cache[path]; old != nil {
		g.bytes -= old.size()
		g.removeLocked(path)
	}
	g.cache[path] = f
	g.order = append(g.order, path)
	g.bytes += f.size()

	current := g.paths[g.index]
	for g.bytes > g.budget {
		victim := ""
		for _, p := range g.order {
			if p != current {
				victim = p
				break
			}
		}
		if victim == "" {
			break
		}
		g.bytes -= g.cache[victim].size()
		g.removeLocked(victim)
	}
}

func (g *gallery) touchLocked(path string) {
	for i, p := range g.order {
		if p == path {
			g.order = append(g.order[:i], g.order[i+1:]...)
			g.order = append(g.order, path)
			return
		}
	}
}

func (g *gallery) removeLocked(path string) {
	delete(g.cache, path)
	for i, p := range g.order {
		if p == path {
			g.order = append(g.order[:i], g.order[i+1:]...)
			return
		}
	}
}
