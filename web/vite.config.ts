import react from '@vitejs/plugin-react';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  // kgd は画面を /viewer/ 以下で配信する
  base: '/viewer/',
  plugins: [react()],
  server: {
    // 開発中は API を手元の kgd へ流す。kgd の allowed_cidrs に 127.0.0.1/32 を足しておくこと
    proxy: { '/viewer/api': 'http://127.0.0.1:8081' },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'scripts/**/*.test.ts'],
    // Task 10 時点ではテストが無いため、以降のタスクがテストを足すまで空集合を許す
    passWithNoTests: true,
  },
});
