<script lang="ts">
	import { router, href } from './lib/router.svelte'
	import { theme } from './lib/theme.svelte'
	import { search } from './lib/search.svelte'
	import { loadSite } from './lib/data'
	import type { Site } from './types'

	import Browse from './components/Browse.svelte'
	import DocumentPage from './components/DocumentPage.svelte'
	import GraphView from './components/GraphView.svelte'
	import Timeline from './components/Timeline.svelte'
	import LazyMount from './components/LazyMount.svelte'

	// The site index gates every view (name/category/hasData lookups); load it once
	// and pass it down. The WASM engine lives inside the `search` store.
	const sitePromise: Promise<Site> = loadSite()

	// Everything lives on one page as stacked sections; the nav jumps between them.
	const sections = [
		{ id: 'sec-browse', label: '一覧・検索' },
		{ id: 'sec-timeline', label: '沿革' },
		{ id: 'sec-graph', label: '参照' },
	] as const

	let activeSection = $state<string>('sec-browse')

	function jump(id: string): void {
		document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
	}

	// Scrollspy for the jump nav: highlight the section currently at the top.
	$effect(() => {
		if (router.route.name !== 'home') return
		let obs: IntersectionObserver | null = null
		const raf = requestAnimationFrame(() => {
			const els = sections
				.map((s) => document.getElementById(s.id))
				.filter((e): e is HTMLElement => e !== null)
			if (els.length === 0) return
			const visible = new Set<string>()
			obs = new IntersectionObserver(
				(entries) => {
					for (const e of entries) {
						const id = (e.target as HTMLElement).id
						if (e.isIntersecting) visible.add(id)
						else visible.delete(id)
					}
					const top = els.find((e) => visible.has(e.id))
					if (top) activeSection = top.id
				},
				{ rootMargin: '-120px 0px -55% 0px', threshold: 0 },
			)
			for (const e of els) obs.observe(e)
		})
		return () => {
			cancelAnimationFrame(raf)
			obs?.disconnect()
		}
	})

	const themeIcon = $derived(theme.value === 'auto' ? '◐' : theme.value === 'light' ? '☀' : '☾')
	const themeLabel = $derived(
		theme.value === 'auto' ? '自動' : theme.value === 'light' ? 'ライト' : 'ダーク',
	)

	let route = $derived(router.route)
</script>

<div class="min-h-dvh">
	<header class="glass sticky top-0 z-50 border-b border-line">
		<div class="mx-auto max-w-5xl px-4 py-3">
			<div class="flex items-center gap-3">
				<a href={href({ name: 'home' })} class="shrink-0 font-bold text-ink">📜 高知大学 規則集</a>

				<!-- Persistent search: typing from a document jumps back to the page results. -->
				<div
					class="flex min-w-0 flex-1 items-center gap-2 rounded-full border border-line bg-card px-3 py-1.5 focus-within:border-accent"
				>
					<span class="shrink-0 text-ink-3" aria-hidden="true">🔍</span>
					<input
						value={search.query}
						oninput={(e) => search.set(e.currentTarget.value)}
						type="search"
						enterkeyhint="search"
						placeholder="全規則を条文検索…"
						aria-label="全文検索"
						class="min-w-0 flex-1 bg-transparent text-sm text-ink placeholder:text-ink-3 focus:outline-none"
					/>
					{#if search.query}
						<button
							type="button"
							onclick={() => search.set('')}
							aria-label="検索語を消去"
							class="shrink-0 rounded px-1 text-ink-3 transition-colors hover:text-ink-2"
						>
							<span aria-hidden="true">✕</span>
						</button>
					{/if}
				</div>

				<button
					type="button"
					onclick={() => theme.cycle()}
					class="shrink-0 rounded-md px-2 py-1 text-ink-2 transition-colors hover:bg-fill"
					title={`テーマ: ${themeLabel}`}
					aria-label={`テーマ切替（現在: ${themeLabel}）`}
				>
					<span aria-hidden="true">{themeIcon}</span>
				</button>
			</div>

			<!-- Section jump nav (only on the one page, not on a document drill-in). -->
			{#if route.name === 'home'}
				<nav class="mt-2 flex items-center gap-1 text-sm" aria-label="セクション">
					{#each sections as s (s.id)}
						<button
							type="button"
							onclick={() => jump(s.id)}
							aria-current={activeSection === s.id ? 'true' : undefined}
							class="rounded-md px-2.5 py-1 transition-colors hover:bg-fill {activeSection === s.id
								? 'bg-fill-strong font-semibold text-ink'
								: 'text-ink-2'}"
						>
							{s.label}
						</button>
					{/each}
				</nav>
			{/if}
		</div>
	</header>

	<main class="mx-auto max-w-5xl px-4 py-6">
		{#await sitePromise}
			<p class="py-20 text-center text-ink-3">読み込み中…</p>
		{:then site}
			{#if route.name === 'doc' && route.code}
				<DocumentPage code={route.code} article={route.article} {site} />
			{:else}
				<!-- One page: browse/search, then the timeline, then the graph. The two
				     heavier sections mount lazily as they scroll into view. -->
				<section id="sec-browse" class="scroll-mt-28">
					<Browse {site} />
				</section>

				<section id="sec-timeline" class="mt-14 scroll-mt-28 border-t border-line pt-8">
					<LazyMount>
						{#snippet children()}
							<Timeline {site} />
						{/snippet}
					</LazyMount>
				</section>

				<section id="sec-graph" class="mt-14 scroll-mt-28 border-t border-line pt-8">
					<LazyMount>
						{#snippet children()}
							<GraphView />
						{/snippet}
					</LazyMount>
				</section>
			{/if}
		{:catch err}
			<p class="py-20 text-center text-ink-3">データの読み込みに失敗しました：{err.message}</p>
		{/await}
	</main>

	<footer class="mx-auto max-w-5xl px-4 py-10 text-center text-xs text-ink-3">
		非公式ビューアー。原典は
		<a href="https://www.kochi-u.ac.jp/education-support/regulations/" target="_blank" rel="noopener">
			高知大学 規則集
		</a>
		を参照してください。
	</footer>
</div>
