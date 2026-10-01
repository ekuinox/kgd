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
  build: {
    // LAN 限定で kgd のバイナリから配信するだけの画面なので、分割の手間よりシンプルさを優先する。
    // MapLibre と Recharts を積むと主チャンクが ~1.8 MB になり、既定の 500 kB 警告は常に
    // 鳴ってしまうため、実測値に余裕を見た閾値まで上げておく。
    chunkSizeWarningLimit: 2000,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'scripts/**/*.test.ts'],
    // Task 10 時点ではテストが無いため、以降のタスクがテストを足すまで空集合を許す
    passWithNoTests: true,
  },
});
