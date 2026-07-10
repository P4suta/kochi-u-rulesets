<script lang="ts">
	import { router, href } from './lib/router.svelte'
	import { theme } from './lib/theme.svelte'
	import { loadSite } from './lib/data'
	import { Engine } from './lib/engine'
	import type { Site } from './types'

	import List from './components/List.svelte'
	import DocumentPage from './components/DocumentPage.svelte'
	import Search from './components/Search.svelte'
	import GraphView from './components/GraphView.svelte'
	import Timeline from './components/Timeline.svelte'
	import Parser from './components/Parser.svelte'

	// The site index gates every view (name/category/hasData lookups), so load it
	// once here and pass it down. The WASM engine (a Web Worker) is created once and
	// shared by Search and Parser; its search index loads lazily on first use.
	const sitePromise: Promise<Site> = loadSite()
	const engine = new Engine()

	const nav = [
		{ name: 'list', label: '一覧' },
		{ name: 'search', label: '検索' },
		{ name: 'timeline', label: '沿革' },
		{ name: 'graph', label: '参照' },
		{ name: 'parser', label: 'パーサ' },
	] as const

	const themeIcon = $derived(
		theme.value === 'auto' ? '◐' : theme.value === 'light' ? '☀' : '☾',
	)
	const themeLabel = $derived(
		theme.value === 'auto' ? '自動' : theme.value === 'light' ? 'ライト' : 'ダーク',
	)

	let route = $derived(router.route)
</script>

<div class="min-h-dvh">
	<header class="glass sticky top-0 z-50 border-b border-line">
		<div class="mx-auto flex max-w-5xl items-center gap-3 px-4 py-3">
			<a href={href({ name: 'list' })} class="shrink-0 font-bold text-ink">
				📜 高知大学 規則集
			</a>
			<nav class="flex flex-1 items-center gap-1 overflow-x-auto hide-scrollbar text-sm">
				{#each nav as item (item.name)}
					<a
						href={href({ name: item.name })}
						class="rounded-md px-2.5 py-1 transition-colors hover:bg-fill {route.name === item.name
							? 'bg-fill-strong font-semibold text-ink'
							: 'text-ink-2'}"
					>
						{item.label}
					</a>
				{/each}
			</nav>
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

	<main class="mx-auto max-w-5xl px-4 py-6">
		{#await sitePromise}
			<p class="py-20 text-center text-ink-3">読み込み中…</p>
		{:then site}
			{#if route.name === 'list'}
				<List {site} />
			{:else if route.name === 'doc' && route.code}
				<DocumentPage code={route.code} article={route.article} {site} />
			{:else if route.name === 'search'}
				<Search {engine} {site} initialQuery={route.query} />
			{:else if route.name === 'graph'}
				<GraphView />
			{:else if route.name === 'timeline'}
				<Timeline {site} />
			{:else if route.name === 'parser'}
				<Parser {engine} {site} />
			{:else}
				<p class="py-20 text-center text-ink-3">ページが見つかりません。</p>
			{/if}
		{:catch err}
			<p class="py-20 text-center text-ink-3">
				データの読み込みに失敗しました：{err.message}
			</p>
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
