import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [sveltekit()],
  server: {
    proxy: {
      '/api/v1': 'http://localhost:8080',
      '/api/v1/image/:id/dump/ws': { target: 'ws://localhost:8080', ws: true },
    },
  },
});
