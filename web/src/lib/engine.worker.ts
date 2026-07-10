/// <reference lib="webworker" />
//
// Off-main-thread home for the WASM search core. The prebuilt `search.idx` (a
// ~200KB decode) is loaded once into an Engine and then queried, so neither the
// load nor the queries block the main thread. The main thread talks to this
// worker through the id-keyed request / reply protocol below (see engine.ts).

import initWasm, { Engine as WasmEngine } from '../wasm/kochi_university_regulations_wasm.js'

type Request =
	| { id: number; type: 'search'; query: string; limit: number }
	| { id: number; type: 'docCount' }

let wasmReady: Promise<void> | null = null
/** Boot the wasm module once; subsequent calls await the same promise. */
function ensureWasm(): Promise<void> {
	if (!wasmReady) wasmReady = initWasm().then(() => undefined)
	return wasmReady
}

let enginePromise: Promise<WasmEngine> | null = null
/** Load `search.idx` once and build the search engine; shared by all queries. */
function ensureEngine(): Promise<WasmEngine> {
	if (!enginePromise) {
		enginePromise = (async () => {
			await ensureWasm()
			const res = await fetch(`${import.meta.env.BASE_URL}search.idx`)
			if (!res.ok) {
				throw new Error(`検索インデックスの取得に失敗しました (HTTP ${res.status})`)
			}
			const bytes = new Uint8Array(await res.arrayBuffer())
			return WasmEngine.fromIndex(bytes)
		})()
	}
	return enginePromise
}

async function handle(msg: Request): Promise<unknown> {
	const engine = await ensureEngine()
	switch (msg.type) {
		case 'search':
			return engine.query(msg.query, msg.limit)
		case 'docCount':
			return engine.docCount()
	}
}

self.onmessage = async (e: MessageEvent<Request>) => {
	const msg = e.data
	try {
		const result = await handle(msg)
		self.postMessage({ id: msg.id, ok: true, result })
	} catch (err) {
		self.postMessage({
			id: msg.id,
			ok: false,
			error: err instanceof Error ? err.message : String(err),
		})
	}
}
