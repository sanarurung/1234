# DiPlay Windows Port

Windows 11 port workspace. The Android project remains untouched.

## Stack
- React + Vite renderer
- Electron main/preload
- Rust sidecar backend (JSON Lines over stdin/stdout)
- Windows SetupAPI for read-only USB Plug and Play enumeration

## Prerequisites
- Windows 11
- Node.js 20+
- Rust stable toolchain (Cargo must be available in PATH)

## Development
From this directory, run:

```powershell
npm install
npm run dev
```

Electron starts the Rust backend with Cargo automatically. The first USB scan lists currently present USB Plug and Play devices that expose VID/PID hardware IDs. It does not open the USB interfaces, change drivers, or perform CarPlay authentication.

## Backend
Build the Rust sidecar manually with:

```powershell
cd backend
cargo build
```

This milestone only inventories USB device metadata. USBMUX, Lockdown, iAP2, MFi, NCM, and AirPlay are not implemented yet. The next milestone must validate which iPhone USB interfaces Windows exposes and which driver owns them before attempting transport I/O.
