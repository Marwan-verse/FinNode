import { defineConfig } from 'vite';

const devHost = process.env.TAURI_DEV_HOST?.trim();
const serverHost = devHost || '0.0.0.0';

export default defineConfig({
  server: {
    host: serverHost,
    port: 5173,
    strictPort: true,
    hmr: devHost && devHost !== '0.0.0.0'
      ? {
          protocol: 'ws',
          host: devHost,
          port: 5173,
        }
      : undefined,
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
