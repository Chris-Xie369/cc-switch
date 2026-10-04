import path from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
export default defineConfig({
  root: __dirname, plugins: [react()],
  resolve: {alias: {'@': path.resolve(__dirname, '../../../src')}},
  css: {postcss: path.resolve(__dirname, '../../..')},
  server: {host: '127.0.0.1', port: 3437, strictPort: true, fs: {allow: [path.resolve(__dirname, '../../..')]}},
});
