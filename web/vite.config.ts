import { defineConfig } from 'vite';
import { resolve } from 'node:path';
export default defineConfig({build:{target:'es2022',rollupOptions:{input:{phone:resolve(__dirname,'phone.html'),admin:resolve(__dirname,'admin.html'),settings:resolve(__dirname,'settings.html')}}}});
