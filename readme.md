<p align="center">
  <img src="assets/banner.png" alt="piclent" width="400">
</p>

<p align="center">
  A tiny image viewer for Windows.
</p>

## Install
Get `piclent-VERSION-setup.exe` in Releases.

## Build

Go 1.21 or newer:

```
go build -trimpath -ldflags="-H windowsgui -s -w" -o piclent.exe .
```

## License
Copyright (c) 2026 Greg and Contributors<br>
MIT, see [license.txt](license.txt).
