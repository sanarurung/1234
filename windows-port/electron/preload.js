const { contextBridge, ipcRenderer } = require("electron");

contextBridge.exposeInMainWorld("diPlay", {
  getStatus: () => ipcRenderer.invoke("app:get-status"),
  listUsbDevices: () => ipcRenderer.invoke("usb:list"),
  inspectUsb: () => ipcRenderer.invoke("usb:inspect"),
  probeUsbmux: () => ipcRenderer.invoke("usbmux:probe"),
  queryLockdownType: () => ipcRenderer.invoke("lockdown:query-type"),
  preparePairing: () => ipcRenderer.invoke("pairing:prepare"),
  requestPairing: () => ipcRenderer.invoke("pairing:pair"),
  validatePairing: () => ipcRenderer.invoke("pairing:validate"),
  startLockdownSession: () => ipcRenderer.invoke("lockdown:start-session"),
  pairingPreflight: () => ipcRenderer.invoke("lockdown:pairing-preflight")
});
