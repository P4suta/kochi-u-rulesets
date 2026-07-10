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

	// The site index gates every view (name/category/hasData lookups); load it once
	// and pass it down. The WASM engine lives inside the `search` store.
	const sitePromise: Promise<Site> = loadSite()

	const tabs = [
		{ name: 'home', label: '一覧・検索' },
		{ name: 'timeline', label: '沿革' },
		{ name: 'graph', label: '参照' },
	] as const

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

				<!-- Persistent search: typing from any tab jumps to the hub with results. -->
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

			<!-- Tab strip (hidden on a document drill-in). -->
			{#if route.name !== 'doc'}
				<nav class="mt-2 flex items-center gap-1 text-sm" aria-label="表示切替">
					{#each tabs as t (t.name)}
						<a
							aria-current={route.name === t.name ? 'page' : undefined}
							href={href({ name: t.name })}
							class="rounded-md px-2.5 py-1 transition-colors hover:bg-fill {route.name === t.name
								? 'bg-fill-strong font-semibold text-ink'
								: 'text-ink-2'}"
						>
							{t.label}
						</a>
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
				<!-- Browse stays mounted across hub/沿革/参照 (its facet + scroll persist);
				     the query itself lives in the store, so it survives regardless. -->
				<div style:display={route.name === 'home' ? '' : 'none'}>
					<Browse {site} />
				</div>
				{#if route.name === 'timeline'}<Timeline {site} />{/if}
				{#if route.name === 'graph'}<GraphView />{/if}
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
