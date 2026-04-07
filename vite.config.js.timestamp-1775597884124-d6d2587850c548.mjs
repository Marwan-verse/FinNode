// vite.config.js
import { defineConfig } from "file:///mnt/d/project/FinNode/FinNode/node_modules/vite/dist/node/index.js";
import { svelte } from "file:///mnt/d/project/FinNode/FinNode/node_modules/@sveltejs/vite-plugin-svelte/src/index.js";
var devHost = process.env.TAURI_DEV_HOST?.trim();
var serverHost = devHost || "0.0.0.0";
var vite_config_default = defineConfig({
  plugins: [svelte()],
  server: {
    host: serverHost,
    port: 5173,
    strictPort: true,
    hmr: devHost && devHost !== "0.0.0.0" ? {
      protocol: "ws",
      host: devHost,
      port: 5173
    } : void 0
  },
  build: {
    outDir: "dist",
    emptyOutDir: true
  }
});
export {
  vite_config_default as default
};
//# sourceMappingURL=data:application/json;base64,ewogICJ2ZXJzaW9uIjogMywKICAic291cmNlcyI6IFsidml0ZS5jb25maWcuanMiXSwKICAic291cmNlc0NvbnRlbnQiOiBbImNvbnN0IF9fdml0ZV9pbmplY3RlZF9vcmlnaW5hbF9kaXJuYW1lID0gXCIvbW50L2QvcHJvamVjdC9GaW5Ob2RlL0Zpbk5vZGVcIjtjb25zdCBfX3ZpdGVfaW5qZWN0ZWRfb3JpZ2luYWxfZmlsZW5hbWUgPSBcIi9tbnQvZC9wcm9qZWN0L0Zpbk5vZGUvRmluTm9kZS92aXRlLmNvbmZpZy5qc1wiO2NvbnN0IF9fdml0ZV9pbmplY3RlZF9vcmlnaW5hbF9pbXBvcnRfbWV0YV91cmwgPSBcImZpbGU6Ly8vbW50L2QvcHJvamVjdC9GaW5Ob2RlL0Zpbk5vZGUvdml0ZS5jb25maWcuanNcIjtpbXBvcnQgeyBkZWZpbmVDb25maWcgfSBmcm9tICd2aXRlJztcclxuaW1wb3J0IHsgc3ZlbHRlIH0gZnJvbSAnQHN2ZWx0ZWpzL3ZpdGUtcGx1Z2luLXN2ZWx0ZSc7XHJcblxyXG5jb25zdCBkZXZIb3N0ID0gcHJvY2Vzcy5lbnYuVEFVUklfREVWX0hPU1Q/LnRyaW0oKTtcclxuY29uc3Qgc2VydmVySG9zdCA9IGRldkhvc3QgfHwgJzAuMC4wLjAnO1xyXG5cclxuZXhwb3J0IGRlZmF1bHQgZGVmaW5lQ29uZmlnKHtcclxuICBwbHVnaW5zOiBbc3ZlbHRlKCldLFxyXG4gIHNlcnZlcjoge1xyXG4gICAgaG9zdDogc2VydmVySG9zdCxcclxuICAgIHBvcnQ6IDUxNzMsXHJcbiAgICBzdHJpY3RQb3J0OiB0cnVlLFxyXG4gICAgaG1yOiBkZXZIb3N0ICYmIGRldkhvc3QgIT09ICcwLjAuMC4wJ1xyXG4gICAgICA/IHtcclxuICAgICAgICAgIHByb3RvY29sOiAnd3MnLFxyXG4gICAgICAgICAgaG9zdDogZGV2SG9zdCxcclxuICAgICAgICAgIHBvcnQ6IDUxNzMsXHJcbiAgICAgICAgfVxyXG4gICAgICA6IHVuZGVmaW5lZCxcclxuICB9LFxyXG4gIGJ1aWxkOiB7XHJcbiAgICBvdXREaXI6ICdkaXN0JyxcclxuICAgIGVtcHR5T3V0RGlyOiB0cnVlLFxyXG4gIH0sXHJcbn0pO1xyXG4iXSwKICAibWFwcGluZ3MiOiAiO0FBQTRRLFNBQVMsb0JBQW9CO0FBQ3pTLFNBQVMsY0FBYztBQUV2QixJQUFNLFVBQVUsUUFBUSxJQUFJLGdCQUFnQixLQUFLO0FBQ2pELElBQU0sYUFBYSxXQUFXO0FBRTlCLElBQU8sc0JBQVEsYUFBYTtBQUFBLEVBQzFCLFNBQVMsQ0FBQyxPQUFPLENBQUM7QUFBQSxFQUNsQixRQUFRO0FBQUEsSUFDTixNQUFNO0FBQUEsSUFDTixNQUFNO0FBQUEsSUFDTixZQUFZO0FBQUEsSUFDWixLQUFLLFdBQVcsWUFBWSxZQUN4QjtBQUFBLE1BQ0UsVUFBVTtBQUFBLE1BQ1YsTUFBTTtBQUFBLE1BQ04sTUFBTTtBQUFBLElBQ1IsSUFDQTtBQUFBLEVBQ047QUFBQSxFQUNBLE9BQU87QUFBQSxJQUNMLFFBQVE7QUFBQSxJQUNSLGFBQWE7QUFBQSxFQUNmO0FBQ0YsQ0FBQzsiLAogICJuYW1lcyI6IFtdCn0K
