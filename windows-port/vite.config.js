import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({ plugins: [react()], root: ".", base: "./", server: { host: "127.0.0.1", port: 5173, strictPort: true, watch: { ignored: ["**/backend/target/**"] } }, build: { outDir: "dist", emptyOutDir: true } });
