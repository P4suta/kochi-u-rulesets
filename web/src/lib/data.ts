import type { Document, Graph, Site } from '../types'

// Fetch helpers for the CLI-generated static data under `public/`. `BASE_URL`
// carries the GitHub Pages sub-path in production and `/` locally. Per-document
// JSON is fetched on demand (and cached) so the list/search views stay light.

const base = import.meta.env.BASE_URL

async function getJson<T>(path: string): Promise<T> {
	const res = await fetch(`${base}${path}`)
	if (!res.ok) throw new Error(`${path} の取得に失敗しました (HTTP ${res.status})`)
	return res.json() as Promise<T>
}

export function loadSite(): Promise<Site> {
	return getJson<Site>('site.json')
}

export function loadGraph(): Promise<Graph> {
	return getJson<Graph>('graph.json')
}

const docCache = new Map<string, Promise<Document>>()

/** Fetch (and memoize) one ruleset's structured document. */
export function loadDoc(code: string): Promise<Document> {
	let cached = docCache.get(code)
	if (!cached) {
		cached = getJson<Document>(`docs/${code}.json`)
		docCache.set(code, cached)
	}
	return cached
}

/** Fetch one ruleset's saved Markdown (for the live-parser diff view). */
export async function loadDocMarkdown(code: string): Promise<string> {
	const res = await fetch(`${base}docs/${code}.md`)
	if (!res.ok) throw new Error(`docs/${code}.md の取得に失敗しました (HTTP ${res.status})`)
	return res.text()
}
