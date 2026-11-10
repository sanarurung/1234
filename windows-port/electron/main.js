import { app, BrowserWindow, ipcMain } from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
function createWindow() {
  const win = new BrowserWindow({ width: 1180, height: 760, minWidth: 900, minHeight: 600, backgroundColor: "#10131a", webPreferences: { preload: path.join(here, "preload.js"), contextIsolation: true, nodeIntegration: false, sandbox: true } });
  if (process.env.VITE_DEV_SERVER_URL) win.loadURL(process.env.VITE_DEV_SERVER_URL);
  else win.loadFile(path.join(here, "../dist/index.html"));
}
ipcMain.handle("app:get-status", async () => ({ app: "DiPlay for Windows", platform: process.platform, backend: "not-started", usb: "not-implemented" }));
app.whenReady().then(() => { createWindow(); app.on("activate", () => { if (BrowserWindow.getAllWindows().length === 0) createWindow(); }); });
app.on("window-all-closed", () => { if (process.platform !== "darwin") app.quit(); });
