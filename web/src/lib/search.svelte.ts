// The full-text search store. Kept outside any component so the query and results
// survive tab switches (browse → 沿革 → browse) and even a drill-in to a document
// and back. It owns the single WASM Engine (a Web Worker; the only WASM user now
// that the live parser is gone), debounces input, guards against out-of-order
// replies, and keeps `#/?q=…` in sync without spamming history.

import type { Hit } from '../types'
import { Engine } from './engine'
import { parseHash } from './router.svelte'

class SearchStore {
	query = $state('')
	results = $state<Hit[]>([])
	loading = $state(false)
	error = $state<string | null>(null)
	docCount = $state<number | null>(null)

	private engine = new Engine()
	private seq = 0
	private timer: ReturnType<typeof setTimeout> | null = null

	constructor() {
		// Seed from a shared/bookmarked #/?q=… link.
		const r = parseHash(location.hash)
		if (r.name === 'home' && r.query) {
			this.query = r.query
			this.schedule(r.query)
		}
		// The "N規則を検索" hint; triggers the lazy index load, degrades quietly.
		this.engine
			.docCount()
			.then((n) => (this.docCount = n))
			.catch(() => {})
	}

	/** Update the query from the search box: debounced search + URL sync. */
	set(q: string): void {
		this.query = q
		this.schedule(q)
		this.syncUrl(q)
	}

	/** Reflect the query in the URL. On the hub, replaceState (silent); elsewhere,
	 *  set the hash so typing anywhere jumps to the hub/results tab. */
	private syncUrl(q: string): void {
		const target = q.trim() ? `#/?q=${encodeURIComponent(q.trim())}` : '#/'
		const onHome =
			location.hash === '' || location.hash === '#/' || location.hash.startsWith('#/?')
		if (onHome) history.replaceState(history.state, '', target)
		else location.hash = target
	}

	private schedule(q: string): void {
		if (this.timer) clearTimeout(this.timer)
		const trimmed = q.trim()
		const seq = ++this.seq // invalidates any in-flight reply from a prior run
		this.error = null

		if (trimmed === '') {
			this.results = []
			this.loading = false
			return
		}

		this.loading = true
		this.timer = setTimeout(() => {
			this.engine
				.search(trimmed, 60)
				.then((hits) => {
					if (seq !== this.seq) return
					this.results = hits
					this.loading = false
				})
				.catch(() => {
					if (seq !== this.seq) return
					this.results = []
					this.error = '検索に失敗しました。時間をおいて再度お試しください。'
					this.loading = false
				})
		}, 200)
	}
}

export const search = new SearchStore()
