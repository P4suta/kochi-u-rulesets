/// <reference lib="webworker" />
//
// Off-main-thread home for the WASM core. Two independent uses of the same
// `crates/wasm` module: `search` (the prebuilt `search.idx` loaded once into an
// `Engine`, then queried) and `parse` (a dropped PDF parsed live). Both the index
// load (a ~200KB decode) and the PDF parse are heavy enough to want off the main
// thread. The main thread talks to this worker through the id-keyed request /
// response protocol below (see engine.ts).

import initWasm, {
	Engine as WasmEngine,
	parse as wasmParse,
} from '../wasm/kochi_university_regulations_wasm.js'

type Request =
	| { id: number; type: 'search'; query: string; limit: number }
	| { id: number; type: 'docCount' }
	| { id: number; type: 'parse'; buffer: ArrayBuffer }

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
	switch (msg.type) {
		case 'search': {
			const engine = await ensureEngine()
			return engine.query(msg.query, msg.limit)
		}
		case 'docCount': {
			const engine = await ensureEngine()
			return engine.docCount()
		}
		case 'parse': {
			await ensureWasm()
			// A malformed PDF panics the module (panic=abort); the panic hook logs it
			// and the id-keyed reply below never resolves for that call — the UI times
			// the parse out and tells the user to reload. Well-formed input returns
			// {title, json, markdown}.
			return wasmParse(new Uint8Array(msg.buffer))
		}
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
