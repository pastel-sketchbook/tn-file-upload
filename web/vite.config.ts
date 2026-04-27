import tailwindcss from '@tailwindcss/vite'
import { TanStackRouterVite } from '@tanstack/router-plugin/vite'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

export default defineConfig({
	plugins: [TanStackRouterVite(), tailwindcss(), react()],
	resolve: {
		alias: {
			'@': new URL('./src', import.meta.url).pathname,
		},
	},
	server: {
		proxy: {
			'/api': {
				target: 'http://[::1]:3001',
				changeOrigin: true,
			},
		},
	},
})
