import { defineConfig } from 'vite'
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import react from '@vitejs/plugin-react'

// https://vitejs.dev/config/
export default defineConfig(({ command }) => {
  const isDev = command === 'serve'
  // Use loro-websocket SimpleServer in dev
  const wsUrl = isDev ? 'ws://localhost:8080' : process.env.LORO_SERVER_WS_URL

  return {
    plugins: [react(), wasm(), topLevelAwait()],
    define: {
      'process.env.NODE_ENV': JSON.stringify(process.env.NODE_ENV),
      // Expose resolved WS URL to the app code
      'import.meta.env.VITE_WS_URL': JSON.stringify(wsUrl),
    },
    server: {
      port: 3000,
    },
  }
})
