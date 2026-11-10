import { contextBridge, ipcRenderer } from "electron";
contextBridge.exposeInMainWorld("diPlay", { getStatus: () => ipcRenderer.invoke("app:get-status") });
