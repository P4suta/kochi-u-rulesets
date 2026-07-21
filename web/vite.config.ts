import { svelte } from '@sveltejs/vite-plugin-svelte'
import tailwindcss from '@tailwindcss/vite'
import { defineConfig } from 'vite'

// On GitHub Pages the site is served from a sub-path named after the repository
// (kept as kochi-u-rulesets — the crates were renamed but the repo was not). The
// WASM asset URL (resolved via import.meta.url) and every fetch of the generated
// data under public/ follow this base automatically. Locally the base is `/`.
export default defineConfig({
	base: process.env.GITHUB_PAGES === 'true' ? '/kochi-u-rulesets/' : '/',
	plugins: [svelte(), tailwindcss()],
	worker: { format: 'es' },
})
