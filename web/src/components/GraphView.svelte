<script lang="ts">
	import type { Graph, GraphEdge } from '../types'
	import { loadGraph } from '../lib/data'
	import { router } from '../lib/router.svelte'

	// The graph describes itself: node/edge labels below use the graph's own data,
	// never site.rulesets, so the picture and its caption can never disagree — hence
	// this view takes no props.

	// ── Data (loadGraph is not memoized in data.ts, but this no-dep effect fires
	//    exactly once, so the fetch happens once). ────────────────────────────────
	let graph = $state<Graph | null>(null)
	let error = $state<string | null>(null)

	$effect(() => {
		loadGraph()
			.then((g) => (graph = g))
			.catch((e) => (error = e instanceof Error ? e.message : String(e)))
	})

	// ── Layout target (a fixed coordinate space; the simulation is normalized into
	//    it so node radii and label sizes stay proportional regardless of spread). ─
	const TW = 920
	const TH = 620
	const PAD = 64

	interface Pt {
		x: number
		y: number
	}

	/** A tiny deterministic spring simulation (no external libs). Nodes start on a
	 *  circle at angle 2πi/n, then 300 iterations of inverse-square repulsion +
	 *  spring attraction along edges + light centering, integrated with a cooling
	 *  step cap. Result is scaled to fit the fixed TW×TH viewBox. O(n²), n≈25. */
	function computeLayout(g: Graph): Map<string, Pt> {
		const nodes = g.nodes
		const n = nodes.length
		if (n === 0) return new Map()

		const idx = new Map(nodes.map((nd, i) => [nd.code, i]))
		const pos: Pt[] = nodes.map((_, i) => {
			const a = (2 * Math.PI * i) / n
			return { x: Math.cos(a) * 240, y: Math.sin(a) * 240 }
		})
		const edges = g.edges
			.map((e) => ({ s: idx.get(e.from), t: idx.get(e.to) }))
			.filter((e): e is { s: number; t: number } => e.s !== undefined && e.t !== undefined)

		const REPULSE = 9000
		const SPRING = 0.045
		const LEN = 95
		const CENTER = 0.01
		const ITERS = 300

		for (let it = 0; it < ITERS; it++) {
			const cooling = 1 - it / ITERS
			const disp: Pt[] = pos.map(() => ({ x: 0, y: 0 }))

			// Pairwise repulsion (inverse-square).
			for (let i = 0; i < n; i++) {
				for (let j = i + 1; j < n; j++) {
					const dx = pos[i].x - pos[j].x
					const dy = pos[i].y - pos[j].y
					const d2 = dx * dx + dy * dy || 0.01
					const d = Math.sqrt(d2)
					const f = REPULSE / d2
					const ux = dx / d
					const uy = dy / d
					disp[i].x += ux * f
					disp[i].y += uy * f
					disp[j].x -= ux * f
					disp[j].y -= uy * f
				}
			}

			// Spring attraction along edges toward the ideal length.
			for (const e of edges) {
				const dx = pos[e.t].x - pos[e.s].x
				const dy = pos[e.t].y - pos[e.s].y
				const d = Math.sqrt(dx * dx + dy * dy) || 0.01
				const f = SPRING * (d - LEN)
				const ux = dx / d
				const uy = dy / d
				disp[e.s].x += ux * f
				disp[e.s].y += uy * f
				disp[e.t].x -= ux * f
				disp[e.t].y -= uy * f
			}

			// Light pull toward the origin so the cloud stays cohesive.
			for (let i = 0; i < n; i++) {
				disp[i].x -= pos[i].x * CENTER
				disp[i].y -= pos[i].y * CENTER
			}

			// Integrate with a cooling per-step cap.
			const cap = 28 * cooling + 2
			for (let i = 0; i < n; i++) {
				const dx = disp[i].x
				const dy = disp[i].y
				const dl = Math.sqrt(dx * dx + dy * dy) || 1
				const m = Math.min(dl, cap)
				pos[i].x += (dx / dl) * m
				pos[i].y += (dy / dl) * m
			}
		}

		// Normalize into the fixed TW×TH box, preserving aspect ratio and centering.
		let minX = Infinity
		let minY = Infinity
		let maxX = -Infinity
		let maxY = -Infinity
		for (const p of pos) {
			minX = Math.min(minX, p.x)
			minY = Math.min(minY, p.y)
			maxX = Math.max(maxX, p.x)
			maxY = Math.max(maxY, p.y)
		}
		const bw = maxX - minX || 1
		const bh = maxY - minY || 1
		const scale = Math.min((TW - 2 * PAD) / bw, (TH - 2 * PAD) / bh)
		const offX = (TW - bw * scale) / 2
		const offY = (TH - bh * scale) / 2

		return new Map(
			nodes.map((nd, i) => [
				nd.code,
				{ x: offX + (pos[i].x - minX) * scale, y: offY + (pos[i].y - minY) * scale },
			]),
		)
	}

	// Memoized: recomputes only when `graph`'s reference changes (once, on load).
	const layout = $derived.by(() => (graph ? computeLayout(graph) : null))

	// Node degree (drives radius) and undirected adjacency (drives hover dimming).
	const degree = $derived.by(() => {
		const d = new Map<string, number>()
		if (graph) for (const e of graph.edges) {
			d.set(e.from, (d.get(e.from) ?? 0) + 1)
			d.set(e.to, (d.get(e.to) ?? 0) + 1)
		}
		return d
	})
	const neighbors = $derived.by(() => {
		const m = new Map<string, Set<string>>()
		const add = (a: string, b: string) => {
			if (!m.has(a)) m.set(a, new Set())
			m.get(a)!.add(b)
		}
		if (graph) for (const e of graph.edges) {
			add(e.from, e.to)
			add(e.to, e.from)
		}
		return m
	})

	let hovered = $state<string | null>(null)

	function nodeRadius(code: string): number {
		return 8 + (degree.get(code) ?? 0) * 1.6
	}
	function nodeDim(code: string): boolean {
		return hovered !== null && code !== hovered && !(neighbors.get(hovered)?.has(code) ?? false)
	}
	function edgeActive(e: GraphEdge): boolean {
		return hovered === null || e.from === hovered || e.to === hovered
	}

	/** Endpoint of a directed edge, pulled back to the rim of the node circle (plus
	 *  a gap so the arrowhead sits just outside the target node). */
	function edgeGeom(e: GraphEdge, pos: Map<string, Pt>) {
		const a = pos.get(e.from)!
		const b = pos.get(e.to)!
		const dx = b.x - a.x
		const dy = b.y - a.y
		const d = Math.sqrt(dx * dx + dy * dy) || 1
		const ux = dx / d
		const uy = dy / d
		const ra = nodeRadius(e.from) + 2
		const rb = nodeRadius(e.to) + 6
		return { x1: a.x + ux * ra, y1: a.y + uy * ra, x2: b.x - ux * rb, y2: b.y - uy * rb }
	}

	/** Short display label: names all share the "高知大学" prefix, so drop it and
	 *  truncate; the full name lives in the <title> tooltip. */
	function shortLabel(name: string): string {
		const s = name.replace(/^高知大学/, '') || name
		return s.length > 11 ? s.slice(0, 10) + '…' : s
	}

	function open(code: string): void {
		router.navigate({ name: 'doc', code })
	}
	function onNodeKey(ev: KeyboardEvent, code: string): void {
		if (ev.key === 'Enter' || ev.key === ' ') {
			ev.preventDefault()
			open(code)
		}
	}
</script>

<section>
	<h1 class="text-xl font-bold text-ink">参照グラフ</h1>
	<p class="mt-1 text-sm text-ink-3">
		規則どうしの引用関係。矢印 <span class="font-mono">A → B</span> は
		<span class="text-ink-2">A が B を参照</span>していることを表します。ノードをクリックすると
		その規則を開きます。
	</p>

	{#if error}
		<p class="mt-8 rounded-card border border-line bg-card p-6 text-center text-sm text-ink-3">
			参照グラフの読み込みに失敗しました：{error}
		</p>
	{:else if !graph || !layout}
		<p class="py-20 text-center text-ink-3">読み込み中…</p>
	{:else}
		<p class="mt-3 text-xs text-ink-3">
			<span class="tabular-nums">{graph.nodes.length}</span> ノード・<span class="tabular-nums"
				>{graph.edges.length}</span
			> 本の参照。
		</p>

		<div
			class="mt-3 overflow-x-auto rounded-card border border-line bg-card p-2 shadow-card hide-scrollbar"
		>
			<svg
				viewBox="0 0 {TW} {TH}"
				preserveAspectRatio="xMidYMid meet"
				class="w-full"
				style="max-height: 620px; touch-action: pan-y;"
				role="img"
				aria-label="規則の参照関係を示す力学配置グラフ"
			>
				<defs>
					<marker
						id="graph-arrow"
						viewBox="0 0 10 10"
						refX="9"
						refY="5"
						markerWidth="7"
						markerHeight="7"
						orient="auto-start-reverse"
					>
						<path d="M0,0 L10,5 L0,10 z" style="fill: var(--color-ink-3);" />
					</marker>
				</defs>

				<!-- Edges -->
				{#each graph.edges as e (e.from + '->' + e.to)}
					{@const g = edgeGeom(e, layout)}
					{@const active = edgeActive(e)}
					<line
						x1={g.x1}
						y1={g.y1}
						x2={g.x2}
						y2={g.y2}
						marker-end="url(#graph-arrow)"
						style="stroke: var(--color-{active
							? 'accent'
							: 'ink-3'}); stroke-width: {1.4 + e.count * 0.8}; opacity: {active
							? 0.9
							: 0.15}; transition: opacity 0.2s, stroke 0.2s;"
					>
						<title>{e.from} → {e.to}（{e.articles.join('、')}）</title>
					</line>
				{/each}

				<!-- Nodes -->
				{#each graph.nodes as nd (nd.code)}
					{@const p = layout.get(nd.code)}
					{#if p}
						{@const r = nodeRadius(nd.code)}
						{@const dim = nodeDim(nd.code)}
						{@const isHub = hovered === nd.code}
						<g
							role="button"
							tabindex="0"
							aria-label={`${nd.name} を開く`}
							class="graph-node"
							style="opacity: {dim ? 0.22 : 1}; transition: opacity 0.2s;"
							onclick={() => open(nd.code)}
							onkeydown={(ev) => onNodeKey(ev, nd.code)}
							onmouseenter={() => (hovered = nd.code)}
							onmouseleave={() => (hovered = null)}
							onfocus={() => (hovered = nd.code)}
							onblur={() => (hovered = null)}
						>
							<title>{nd.name}（{nd.code}）</title>
							<circle
								cx={p.x}
								cy={p.y}
								r={r}
								style="fill: var(--color-{(degree.get(nd.code) ?? 0) > 0
									? 'accent'
									: 'fill-strong'}); stroke: var(--color-{isHub
									? 'accent'
									: 'line'}); stroke-width: {isHub ? 2.5 : 1.25};"
							/>
							<text
								x={p.x}
								y={p.y + r + 13}
								text-anchor="middle"
								style="fill: var(--color-ink-2); font-size: 12px; pointer-events: none;"
							>
								{shortLabel(nd.name)}
							</text>
						</g>
					{/if}
				{/each}
			</svg>
		</div>

		<!-- Legend -->
		<div class="mt-3 flex flex-wrap items-center gap-x-5 gap-y-2 text-xs text-ink-3">
			<span class="inline-flex items-center gap-1.5">
				<svg width="34" height="10" aria-hidden="true">
					<defs>
						<marker
							id="legend-arrow"
							viewBox="0 0 10 10"
							refX="9"
							refY="5"
							markerWidth="7"
							markerHeight="7"
							orient="auto-start-reverse"
						>
							<path d="M0,0 L10,5 L0,10 z" style="fill: var(--color-ink-3);" />
						</marker>
					</defs>
					<line
						x1="1"
						y1="5"
						x2="26"
						y2="5"
						marker-end="url(#legend-arrow)"
						style="stroke: var(--color-accent); stroke-width: 2;"
					/>
				</svg>
				A → B ＝ A が B を参照
			</span>
			<span class="inline-flex items-center gap-1.5">
				<span class="inline-block h-3 w-3 rounded-full bg-accent"></span>被参照・参照あり
			</span>
			<span class="inline-flex items-center gap-1.5">
				<span class="inline-block h-3 w-3 rounded-full bg-fill-strong"></span>孤立（参照なし）
			</span>
			<span>線の太さは参照回数に対応します。</span>
		</div>

		<!-- Textual fallback so the relationships are not SVG-only. -->
		<div class="mt-6">
			<h2 class="text-sm font-semibold text-ink">参照一覧</h2>
			{#if graph.edges.length === 0}
				<p class="mt-2 text-sm text-ink-3">規則間の参照は検出されていません。</p>
			{:else}
				<ul class="mt-2 grid gap-1.5 text-sm">
					{#each graph.edges as e (e.from + '->' + e.to)}
						{@const fromName = graph.nodes.find((n) => n.code === e.from)?.name ?? e.from}
						{@const toName = graph.nodes.find((n) => n.code === e.to)?.name ?? e.to}
						<li class="text-ink-2">
							<button
								type="button"
								class="text-accent hover:underline"
								onclick={() => open(e.from)}>{fromName}</button
							>
							<span class="mx-1 text-ink-3" aria-hidden="true">→</span>
							<button
								type="button"
								class="text-accent hover:underline"
								onclick={() => open(e.to)}>{toName}</button
							>
							<span class="text-ink-3">（{e.articles.join('、')}）</span>
						</li>
					{/each}
				</ul>
			{/if}
		</div>
	{/if}
</section>

<style>
	.graph-node {
		cursor: pointer;
		outline: none;
	}
	.graph-node:focus-visible circle {
		stroke: var(--color-accent);
		stroke-width: 3;
	}
</style>
