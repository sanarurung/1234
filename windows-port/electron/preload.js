const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("diPlay", {
  getStatus: () => ipcRenderer.invoke("app:get-status"),
  listUsbDevices: () => ipcRenderer.invoke("usb:list"),
  inspectUsb: () => ipcRenderer.invoke("usb:inspect")
});
