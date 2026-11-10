import { app, BrowserWindow, ipcMain } from "electron";
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const backendDir = path.join(here, "../backend");
let backendProcess = null;
let backendReady = false;
let backendError = "";
const pending = [];

function startBackend() {
  if (process.platform !== "win32") {
    backendError = "USB backend is currently Windows-only.";
    return;
  }
  backendProcess = spawn("cargo", ["run", "--quiet", "--manifest-path", path.join(backendDir, "Cargo.toml")], {
    cwd: backendDir,
    windowsHide: true,
    stdio: ["pipe", "pipe", "pipe"]
  });
  const output = createInterface({ input: backendProcess.stdout });
  output.on("line", (line) => {
    try {
      const message = JSON.parse(line);
      if (message.event === "backend" && message.status === "ready") {
        backendReady = true;
        return;
      }
      const waiter = pending.shift();
      if (waiter) waiter.resolve(message);
    } catch {
      backendError = "Invalid response from Rust backend.";
    }
  });
  backendProcess.stderr.on("data", (chunk) => { backendError = chunk.toString().trim(); });
  backendProcess.on("error", (error) => {
    backendReady = false;
    backendError = error.message;
  });
  backendProcess.on("exit", (code) => {
    backendReady = false;
    if (code && code !== 0) backendError = backendError || "Rust backend exited with code " + code;
  });
}

function sendBackendCommand(command) {
  return new Promise((resolve, reject) => {
    if (!backendProcess || !backendReady || !backendProcess.stdin.writable) {
      reject(new Error(backendError || "Rust backend is still starting. Try again in a moment."));
      return;
    }
    pending.push({ resolve, reject });
    backendProcess.stdin.write(command + "\n", (error) => {
      if (error) {
        const waiter = pending.pop();
        waiter?.reject(error);
      }
    });
  });
}

function createWindow() {
  const win = new BrowserWindow({
    width: 1180, height: 760, minWidth: 900, minHeight: 600,
    backgroundColor: "#10131a",
    webPreferences: {
      preload: path.join(here, "preload.js"),
      contextIsolation: true, nodeIntegration: false, sandbox: true
    }
  });
  if (!app.isPackaged) win.loadURL("http://127.0.0.1:5173");
  else win.loadFile(path.join(here, "../dist/index.html"));
}

ipcMain.handle("app:get-status", async () => ({
  app: "DiPlay for Windows",
  platform: process.platform,
  backend: backendReady ? "ready" : (backendError ? "error" : "starting"),
  backendError
}));
ipcMain.handle("usb:list", async () => sendBackendCommand("usb:list"));

app.whenReady().then(() => {
  startBackend();
  createWindow();
  app.on("activate", () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); });
});
app.on("before-quit", () => {
  if (backendProcess && !backendProcess.killed) backendProcess.kill();
});
app.on("window-all-closed", () => { if (process.platform !== "darwin") app.quit(); });
