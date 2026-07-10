import type { Hit, Parsed } from '../types'

/** A worker reply: `id` echoes the request; `ok` gates `result` vs `error`. */
type WorkerReply =
	| { id: number; ok: true; result: unknown }
	| { id: number; ok: false; error: string }

/**
 * The browser-side facade over the WASM core, which lives in a Web Worker
 * (engine.worker.ts) so the one-time index decode and each live PDF parse never
 * block the main thread. A single monotonic `id` correlates each request with its
 * reply through the `pending` map; the worker echoes the `id` back.
 *
 * Construction is cheap (it only spawns the worker); the search index is loaded
 * lazily inside the worker on the first `search`/`docCount` call, so nothing
 * gates first paint.
 */
export class Engine {
	private readonly worker: Worker
	private seq = 0
	private readonly pending = new Map<
		number,
		{ resolve: (v: unknown) => void; reject: (e: Error) => void }
	>()

	constructor() {
		this.worker = new Worker(new URL('./engine.worker.ts', import.meta.url), {
			type: 'module',
		})
		this.worker.onmessage = (e: MessageEvent<WorkerReply>) => {
			const reply = e.data
			const p = this.pending.get(reply.id)
			if (!p) return
			this.pending.delete(reply.id)
			if (reply.ok) p.resolve(reply.result)
			else p.reject(new Error(reply.error))
		}
	}

	private send(payload: Record<string, unknown>, transfer: Transferable[] = []): Promise<unknown> {
		const id = ++this.seq
		return new Promise((resolve, reject) => {
			this.pending.set(id, { resolve, reject })
			this.worker.postMessage({ id, ...payload }, transfer)
		})
	}

	/** Ranked folding full-text search; returns up to `limit` article hits. */
	search(query: string, limit = 60): Promise<Hit[]> {
		return this.send({ type: 'search', query, limit }) as Promise<Hit[]>
	}

	/** Number of documents in the search index (for a "N規則を検索" hint). */
	docCount(): Promise<number> {
		return this.send({ type: 'docCount' }) as Promise<number>
	}

	/**
	 * Parse a dropped PDF's bytes live. The buffer is transferred (zero-copy);
	 * ownership moves to the worker, so the caller must not touch it afterwards.
	 */
	parse(buffer: ArrayBuffer): Promise<Parsed> {
		return this.send({ type: 'parse', buffer }, [buffer]) as Promise<Parsed>
	}
}
