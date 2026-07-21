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

	// Everything is one page of stacked sections; a sticky ToC (like a document's)
	// gives orientation and jump-to, scroll-synced.
	const sections = [
		{ id: 'sec-browse', label: '一覧・検索' },
		{ id: 'sec-timeline', label: '沿革' },
		{ id: 'sec-graph', label: '参照' },
	] as const

	let activeSection = $state<string>('sec-browse')

	function jump(id: string): void {
		document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' })
	}

	// Scrollspy — highlight the section currently at the top of the viewport.
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
				{ rootMargin: '-72px 0px -60% 0px', threshold: 0 },
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
		<div class="mx-auto flex max-w-6xl items-center gap-3 px-4 py-3">
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
	</header>

	<main class="mx-auto max-w-6xl px-4 py-6">
		{#await sitePromise}
			<p class="py-20 text-center text-ink-3">読み込み中…</p>
		{:then site}
			{#if route.name === 'doc' && route.code}
				<DocumentPage code={route.code} article={route.article} {site} />
			{:else}
				<div class="lg:grid lg:grid-cols-[12rem_minmax(0,1fr)] lg:gap-8">
					<!-- Sticky ToC rail (wide screens): the section index, scroll-synced. -->
					<aside class="hidden lg:block">
						<nav class="sticky top-20" aria-label="目次">
							<p class="mb-2 text-xs font-semibold uppercase tracking-wide text-ink-3">目次</p>
							<ul class="space-y-0.5">
								{#each sections as s (s.id)}
									<li>
										<button
											type="button"
											onclick={() => jump(s.id)}
											aria-current={activeSection === s.id ? 'true' : undefined}
											class="block w-full rounded px-2 py-1 text-left text-sm transition-colors hover:bg-fill {activeSection ===
											s.id
												? 'bg-fill-strong font-medium text-ink'
												: 'text-ink-2'}"
										>
											{s.label}
										</button>
									</li>
								{/each}
							</ul>
						</nav>
					</aside>

					<!-- One page: browse/search, then the timeline, then the graph. The two
					     heavier sections mount lazily as they scroll into view. -->
					<div class="min-w-0">
						<section id="sec-browse" class="scroll-mt-20">
							<Browse {site} />
						</section>

						<section id="sec-timeline" class="mt-14 scroll-mt-20 border-t border-line pt-8">
							<LazyMount>
								{#snippet children()}
									<Timeline {site} />
								{/snippet}
							</LazyMount>
						</section>

						<section id="sec-graph" class="mt-14 scroll-mt-20 border-t border-line pt-8">
							<LazyMount>
								{#snippet children()}
									<GraphView />
								{/snippet}
							</LazyMount>
						</section>
					</div>
				</div>
			{/if}
		{:catch err}
			<p class="py-20 text-center text-ink-3">データの読み込みに失敗しました：{err.message}</p>
		{/await}
	</main>

	<footer class="mx-auto max-w-6xl px-4 py-10 text-center text-xs text-ink-3">
		非公式ビューアー。原典は
		<a href="https://www.kochi-u.ac.jp/education-support/regulations/" target="_blank" rel="noopener">
			高知大学 規則集
		</a>
		を参照してください。
	</footer>
</div>
