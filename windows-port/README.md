# DiPlay Windows Port

Windows 11 port workspace. The Android project remains untouched.

## Stack
- React + Vite renderer
- Electron main/preload
- Rust sidecar backend (JSON Lines over stdin/stdout)

## Prerequisites
- Node.js 20+
- Rust stable toolchain
- Windows 11

## Development
Run `npm install` then `npm run dev` in this directory.

The first milestone is USB device discovery and safe device metadata display. No CarPlay, Lockdown, iAP2, or MFi authentication is implemented yet.

## Backend
Build the Rust sidecar separately with `cd backend; cargo build`.

The Electron main process currently reports backend status as a scaffold. USB access will be implemented only after selecting and validating the Windows USB transport and driver strategy.
