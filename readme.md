# piclent
A tiny image viewer for Windows.

## Install
Get `piclent-VERSION-setup.exe` in Releases.

## Build

Go 1.21 or newer:

```
go build -ldflags="-H windowsgui" -o piclent.exe .
```

The exe icon comes from `assets/icon.ico` through a `.syso` file that the CI build generates with `rsrc`.

## License
Copyright (c) 2026 Greg and Contributors<br>
MIT, see [license.txt](license.txt).
