<script lang="ts">
	// The live drag-and-drop PDF parser. It runs the *same* WASM core that generated
	// the site's saved data, so a dropped regulation is parsed in the browser to output
	// that is byte-identical to the native build — this view proves that "one core"
	// spine by diffing the live Markdown against the saved corpus.
	import type { Document, Parsed, Site } from '../types'
	import type { Engine } from '../lib/engine'
	import { loadDocMarkdown } from '../lib/data'
	import DocumentView from './DocumentView.svelte'

	let { engine, site }: { engine: Engine; site: Site } = $props()

	type Tab = 'formatted' | 'markdown' | 'json'

	type Diff =
		| { kind: 'loading' }
		| { kind: 'match' }
		| { kind: 'differ'; firstLine: number | null; parsedLines: number; savedLines: number; name: string }
		| { kind: 'no_corpus' }
		| { kind: 'unavailable' }

	// The WASM module aborts (panic=abort) on a malformed PDF and the worker reply
	// never resolves; a race timeout catches that. Because the Engine/worker is shared
	// and owned by App (we only receive it as a prop), a single trap kills the core for
	// the rest of the page — so `trapped` is sticky and short-circuits later drops.
	const PARSE_TIMEOUT_MS = 15_000
	const TIMED_OUT = Symbol('parse-timeout')

	let dragging = $state(false)
	let parsing = $state(false)
	let trapped = $state(false)
	let error = $state<string | null>(null)

	let fileName = $state('')
	let fileSize = $state(0)
	let doc = $state<Document | null>(null)
	let parsed = $state<Parsed | null>(null)
	let tab = $state<Tab>('formatted')
	let diff = $state<Diff | null>(null)

	// Non-reactive drag counter: dragenter/dragleave fire per child element, so a plain
	// boolean flickers when the cursor crosses inner nodes. Counting nesting depth keeps
	// the highlight stable until the pointer truly leaves the dropzone.
	let dragDepth = 0

	const tabs: { id: Tab; label: string }[] = [
		{ id: 'formatted', label: '整形表示' },
		{ id: 'markdown', label: 'Markdown' },
		{ id: 'json', label: 'JSON' },
	]

	function parseWithTimeout(buffer: ArrayBuffer): Promise<Parsed> {
		return new Promise((resolve, reject) => {
			const timer = setTimeout(() => reject(TIMED_OUT), PARSE_TIMEOUT_MS)
			engine.parse(buffer).then(
				(v) => {
					clearTimeout(timer)
					resolve(v)
				},
				(e) => {
					clearTimeout(timer)
					reject(e)
				},
			)
		})
	}

	async function computeDiff(p: Parsed): Promise<void> {
		// Match the parsed title to a saved ruleset: exact name first (so a loose
		// substring can't shadow a real hit), then a two-way `includes` fallback.
		let entry = site.rulesets.find((r) => r.name === p.title)
		if (!entry) {
			entry = site.rulesets.find((r) => r.name.includes(p.title) || p.title.includes(r.name))
		}
		if (!entry || !entry.hasData) {
			diff = { kind: 'no_corpus' }
			return
		}
		diff = { kind: 'loading' }
		try {
			const saved = await loadDocMarkdown(entry.code)
			if (p.markdown === saved) {
				diff = { kind: 'match' }
				return
			}
			const a = p.markdown.split('\n')
			const b = saved.split('\n')
			const max = Math.max(a.length, b.length)
			let firstLine: number | null = null
			for (let i = 0; i < max; i++) {
				if (a[i] !== b[i]) {
					firstLine = i
					break
				}
			}
			diff = { kind: 'differ', firstLine, parsedLines: a.length, savedLines: b.length, name: entry.name }
		} catch {
			diff = { kind: 'unavailable' }
		}
	}

	async function handleFile(file: File | null | undefined): Promise<void> {
		if (!file) return
		if (trapped) {
			error = 'パーサは前回の解析で停止したままです。ページを再読み込みしてからお試しください。'
			return
		}
		if (parsing) return

		// Fresh slate for every drop, so a failed or subsequent parse never shows stale output.
		error = null
		doc = null
		parsed = null
		diff = null
		tab = 'formatted'
		fileName = file.name
		fileSize = file.size // read before the buffer is transferred (neutered) into the worker
		parsing = true

		// The parse call is the only step that can poison the worker. With
		// panic=abort a malformed PDF aborts the wasm module — that surfaces here
		// either as a fast RuntimeError rejection OR (if it hangs) as the timeout.
		// BOTH leave the shared module unusable, so `trapped` is set on ANY parse
		// failure, not just the timeout, and later drops short-circuit.
		let result: Parsed
		try {
			const buffer = await file.arrayBuffer()
			result = await parseWithTimeout(buffer)
		} catch (e) {
			parsing = false
			doc = null
			parsed = null
			trapped = true
			error =
				e === TIMED_OUT
					? 'PDFの解析がタイムアウトしました。パーサが停止したため、ページを再読み込みしてからお試しください。'
					: 'PDFを解析できませんでした。破損しているか対応外の形式のため、パーサが停止しました。ページを再読み込みしてから別のPDFをお試しください。'
			return
		}

		// Parse succeeded; the remaining work is pure JS and never poisons the module.
		try {
			parsed = result
			doc = JSON.parse(result.json) as Document
			parsing = false
			await computeDiff(result)
		} catch (e) {
			parsing = false
			doc = null
			parsed = null
			error = `解析結果の処理に失敗しました：${e instanceof Error ? e.message : String(e)}`
		}
	}

	function onDrop(e: DragEvent): void {
		e.preventDefault()
		dragDepth = 0
		dragging = false
		const file = e.dataTransfer?.files?.[0]
		if (file) void handleFile(file)
	}

	function onDragEnter(e: DragEvent): void {
		e.preventDefault()
		dragDepth += 1
		dragging = true
	}

	function onDragLeave(e: DragEvent): void {
		e.preventDefault()
		dragDepth -= 1
		if (dragDepth <= 0) {
			dragDepth = 0
			dragging = false
		}
	}

	function onInput(e: Event): void {
		const input = e.currentTarget as HTMLInputElement
		void handleFile(input.files?.[0])
		input.value = '' // allow re-selecting the same file
	}

	function fmtSize(bytes: number): string {
		if (bytes < 1024) return `${bytes} B`
		if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
		return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
	}
</script>

<section>
	<h1 class="text-xl font-bold text-ink">ライブパーサ</h1>
	<p class="mt-1 text-sm text-ink-3">
		規則のPDFをここにドロップすると、このサイトのデータを生成したものと同じコア（Rust/WASM）でブラウザ内で解析します。出力はサイトのビルド結果とバイト単位で一致します。
	</p>

	<!-- Dropzone -->
	<div
		role="region"
		aria-label="PDFドロップ領域"
		ondrop={onDrop}
		ondragenter={onDragEnter}
		ondragleave={onDragLeave}
		ondragover={(e) => e.preventDefault()}
		class="mt-5 rounded-card border-2 border-dashed p-8 text-center transition-colors {dragging
			? 'border-accent bg-accent/10'
			: 'border-line bg-card'}"
	>
		<div class="pointer-events-none flex flex-col items-center gap-2">
			<span aria-hidden="true" class="text-3xl">📄</span>
			<p class="text-sm text-ink-2">
				{#if dragging}PDFをドロップして解析{:else}PDFをドラッグ＆ドロップ{/if}
			</p>
			<p class="text-xs text-ink-3">または</p>
		</div>
		<label
			class="mt-1 inline-flex cursor-pointer items-center gap-2 rounded-full border border-accent bg-accent px-4 py-2 text-sm font-medium text-on-accent transition-transform hover:-translate-y-0.5 {parsing
				? 'pointer-events-none opacity-60'
				: ''}"
		>
			ファイルを選択
			<input
				type="file"
				accept="application/pdf"
				class="sr-only"
				disabled={parsing || trapped}
				oninput={onInput}
			/>
		</label>
		{#if fileName}
			<p class="mt-3 font-mono text-xs text-ink-3">
				{fileName}{#if fileSize > 0}<span class="tabular-nums">（{fmtSize(fileSize)}）</span>{/if}
			</p>
		{/if}
	</div>

	<!-- Parsing spinner -->
	{#if parsing}
		<div class="mt-5 flex items-center justify-center gap-3 rounded-card border border-line bg-card p-6 text-sm text-ink-2">
			<span
				aria-hidden="true"
				class="inline-block size-5 animate-spin rounded-full border-2 border-line border-t-accent"
			></span>
			解析中…（最大{PARSE_TIMEOUT_MS / 1000}秒）
		</div>
	{/if}

	<!-- Error -->
	{#if error}
		<div
			role="alert"
			class="mt-5 rounded-card border border-line bg-fill p-4 text-sm text-ink-2"
		>
			<span class="font-semibold text-ink">解析できませんでした</span>
			<p class="mt-1">{error}</p>
		</div>
	{/if}

	<!-- Result -->
	{#if doc && parsed && !parsing}
		<!-- Diff badge vs the saved corpus -->
		{#if diff}
			<div class="mt-5">
				{#if diff.kind === 'loading'}
					<p class="text-xs text-ink-3">保存版と照合中…</p>
				{:else if diff.kind === 'match'}
					<div class="inline-flex items-center gap-2 rounded-full border border-accent bg-accent/10 px-3 py-1.5 text-sm font-medium text-accent">
						保存版と完全一致 ✓（バイト一致）
					</div>
				{:else if diff.kind === 'differ'}
					<div class="rounded-card border border-line bg-fill p-4 text-sm text-ink-2">
						<p class="font-semibold text-ink">保存版（{diff.name}）と差分あり</p>
						<p class="mt-1 tabular-nums">
							{#if diff.firstLine !== null}
								最初に相違する行：{diff.firstLine + 1}行目。
							{/if}
							行数：解析結果 {diff.parsedLines}行 / 保存版 {diff.savedLines}行。
						</p>
					</div>
				{:else if diff.kind === 'no_corpus'}
					<p class="text-sm text-ink-3">コーパス外のPDF（差分対象なし）</p>
				{:else if diff.kind === 'unavailable'}
					<p class="text-sm text-ink-3">保存版の取得に失敗したため差分を表示できません。</p>
				{/if}
			</div>
		{/if}

		<!-- Segmented view control -->
		<div class="mt-4 flex flex-wrap gap-2">
			{#each tabs as t (t.id)}
				<button
					type="button"
					onclick={() => (tab = t.id)}
					class="rounded-full border px-3 py-1 text-sm transition-colors {tab === t.id
						? 'border-accent bg-accent text-on-accent'
						: 'border-line text-ink-2 hover:bg-fill'}"
				>
					{t.label}
				</button>
			{/each}
		</div>

		<div class="mt-4 rounded-card border border-line bg-card p-5 shadow-card">
			{#if tab === 'formatted'}
				<DocumentView {doc} />
			{:else if tab === 'markdown'}
				<pre class="whitespace-pre-wrap font-serif text-sm text-ink">{parsed.markdown}</pre>
			{:else}
				<pre class="overflow-x-auto font-mono text-xs text-ink">{parsed.json}</pre>
			{/if}
		</div>
	{/if}
</section>
