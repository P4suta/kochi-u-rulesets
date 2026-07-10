<script lang="ts">
	// The saved-document reading view for one ruleset: a table of contents built by
	// walking the body tree, the shared DocumentView renderer, and a reference
	// adjacency panel from graph.json. Deep links (#/<code>/<条>) scroll to and
	// briefly emphasize the target article. The DocumentPage instance is reused
	// across codes (App keys the view only by route name), so the document reloads
	// whenever `code` changes — and only then, never on an in-page article jump.
	import type { BodyNode, Document, Graph, Site } from '../types'
	import { loadDoc, loadGraph } from '../lib/data'
	import { href } from '../lib/router.svelte'
	import { articleLabel, branchedLabel } from '../lib/format'
	import DocumentView from './DocumentView.svelte'

	let { code, article, site }: { code: string; article?: string; site: Site } = $props()

	let doc = $state<Document | null>(null)
	let graph = $state<Graph | null>(null)
	let error = $state<string | null>(null)

	// The reading column (holds DocumentView's <article id> elements), and the
	// article the reader is currently on — tracked by scroll via IntersectionObserver.
	let contentEl = $state<HTMLElement>()
	let activeArticle = $state<string | undefined>(undefined)
	// The ToC highlights the scroll position once we have one, else the deep-link target.
	const current = $derived(activeArticle ?? article)

	// The site index resolves a ruleset `code` to its display name and origin PDF.
	const entry = $derived(site.rulesets.find((r) => r.code === code))
	const nameByCode = $derived(new Map(site.rulesets.map((r) => [r.code, r.name])))
	const nameOf = (c: string): string => nameByCode.get(c) ?? c

	const outgoing = $derived(graph ? graph.edges.filter((e) => e.from === code) : [])
	const incoming = $derived(graph ? graph.edges.filter((e) => e.to === code) : [])

	// Load effect — depends on `code` ONLY. An article deep-link changes `article`,
	// not `code`, so it must never touch `article` here or every jump would refetch.
	$effect(() => {
		const requested = code
		let active = true
		doc = null
		graph = null
		error = null
		Promise.all([loadDoc(requested), loadGraph()])
			.then(([d, g]) => {
				if (!active) return
				doc = d
				graph = g
			})
			.catch((e) => {
				if (!active) return
				error = e instanceof Error ? e.message : String(e)
			})
		return () => {
			active = false
		}
	})

	// Scroll/emphasize effect — depends on `article` and the rendered `doc`. Runs on
	// first load (when doc arrives with an article set) and on every TOC jump.
	$effect(() => {
		const target = article
		const ready = doc // re-run once the article elements have mounted
		if (!target || !ready) return

		let flashed: HTMLElement | null = null
		const raf = requestAnimationFrame(() => {
			const el = document.getElementById(target)
			if (!el) return
			// html{scroll-padding-top} + article{scroll-mt} clear the sticky header.
			el.scrollIntoView({ block: 'start' })
			el.style.transition = 'background-color 300ms ease'
			el.style.backgroundColor = 'var(--color-fill-strong)'
			el.style.borderRadius = '0.5rem'
			flashed = el
		})
		const timer = window.setTimeout(() => {
			if (flashed) flashed.style.backgroundColor = 'transparent'
		}, 1400)

		return () => {
			cancelAnimationFrame(raf)
			window.clearTimeout(timer)
			if (flashed) {
				flashed.style.backgroundColor = ''
				flashed.style.borderRadius = ''
			}
		}
	})

	// Scrollspy — depends on `doc` (article elements must be mounted) and `contentEl`.
	// Tracks which article the reader is on so the ToC can follow. rootMargin trims
	// the sticky header off the top and treats only the top ~40% as "current". Does
	// not touch the URL (no history noise); highlight only.
	$effect(() => {
		const ready = doc
		const root = contentEl
		if (!ready || !root) return

		let obs: IntersectionObserver | null = null
		const raf = requestAnimationFrame(() => {
			const articles = [...root.querySelectorAll<HTMLElement>('article[id]')]
			if (articles.length === 0) return
			const visible = new Set<string>()
			obs = new IntersectionObserver(
				(entries) => {
					for (const e of entries) {
						const id = (e.target as HTMLElement).id
						if (e.isIntersecting) visible.add(id)
						else visible.delete(id)
					}
					// The top-most visible article in document order is the current one.
					const top = articles.find((a) => visible.has(a.id))
					if (top) activeArticle = top.id
				},
				{ rootMargin: '-80px 0px -60% 0px', threshold: 0 },
			)
			for (const a of articles) obs.observe(a)
		})

		return () => {
			cancelAnimationFrame(raf)
			obs?.disconnect()
		}
	})
</script>

<!-- Recursive table-of-contents entry: chapters/sections nest, articles link. -->
{#snippet tocNode(node: BodyNode)}
	{#if node.type === 'chapter' || node.type === 'section'}
		<li class="mt-2 first:mt-0">
			<p
				class={node.type === 'chapter'
					? 'font-semibold text-ink'
					: 'text-sm font-medium text-ink-2'}
			>
				{branchedLabel(node.number, node.type === 'chapter' ? '章' : '節')}
				<span class="ml-1">{node.title}</span>
			</p>
			<ul class="ml-3 border-l border-line-soft pl-3">
				{#each node.children as child, i (i)}
					{@render tocNode(child)}
				{/each}
			</ul>
		</li>
	{:else}
		{@const label = articleLabel(node.number)}
		<li>
			<a
				href={href({ name: 'doc', code, article: label })}
				aria-current={current === label ? 'true' : undefined}
				class="block rounded px-2 py-0.5 text-sm transition-colors hover:bg-fill {current === label
					? 'bg-fill-strong font-medium text-ink'
					: 'text-ink-2'}"
			>
				<span class="text-accent tabular-nums">{label}</span>
				{#if node.title}<span class="text-ink-3">（{node.title}）</span>{/if}
			</a>
		</li>
	{/if}
{/snippet}

{#snippet tocList(d: Document)}
	<p class="mb-2 text-xs font-semibold uppercase tracking-wide text-ink-3">目次</p>
	<ul class="space-y-0.5">
		{#each d.body as node, i (i)}
			{@render tocNode(node)}
		{/each}
	</ul>
{/snippet}

<!-- One direction of the reference adjacency panel. -->
{#snippet refGroup(title: string, edges: Graph['edges'], dir: 'out' | 'in')}
	<div>
		<h3 class="text-sm font-semibold text-ink-2">{title}</h3>
		{#if edges.length === 0}
			<p class="mt-1 text-sm text-ink-3">参照なし</p>
		{:else}
			<ul class="mt-2 space-y-2">
				{#each edges as edge (dir === 'out' ? edge.to : edge.from)}
					{@const other = dir === 'out' ? edge.to : edge.from}
					<li>
						<a
							href={href({ name: 'doc', code: other })}
							class="font-medium text-accent hover:underline"
						>
							{nameOf(other)}
						</a>
						{#if edge.articles.length > 0}
							<span class="ml-1 text-xs text-ink-3 tabular-nums">
								{edge.articles.join('、')}
							</span>
						{/if}
					</li>
				{/each}
			</ul>
		{/if}
	</div>
{/snippet}

<div>
	<!-- Header: breadcrumb back to the list and a link to the origin PDF. -->
	<div class="flex flex-wrap items-center justify-between gap-2">
		<a href={href({ name: 'home' })} class="text-sm text-ink-2 hover:text-accent">
			← 一覧
		</a>
		{#if entry}
			<a
				href={entry.sourceUrl}
				target="_blank"
				rel="noopener"
				class="text-sm text-accent hover:underline"
			>
				原典PDFを開く ↗
			</a>
		{/if}
	</div>

	{#if error}
		<div class="rounded-card border border-line bg-card p-6 shadow-card mt-6 text-center">
			<p class="text-ink-2">規則の読み込みに失敗しました。</p>
			<p class="mt-1 text-sm text-ink-3">{error}</p>
			<a href={href({ name: 'home' })} class="mt-3 inline-block text-sm text-accent hover:underline">
				一覧に戻る
			</a>
		</div>
	{:else if !doc}
		<p class="py-20 text-center text-ink-3">読み込み中…</p>
	{:else}
		<!-- Narrow screens: the TOC collapses above the reading column. -->
		<details class="mt-5 rounded-card border border-line bg-card p-4 lg:hidden">
			<summary class="cursor-pointer text-sm font-semibold text-ink-2">目次</summary>
			<nav class="mt-3">
				{@render tocList(doc)}
			</nav>
		</details>

		<div class="mt-6 lg:grid lg:grid-cols-[15rem_minmax(0,1fr)] lg:gap-8">
			<!-- Wide screens: a sticky side table of contents. -->
			<aside class="hidden lg:block">
				<nav
					class="sticky top-20 max-h-[calc(100dvh-6rem)] overflow-y-auto hide-scrollbar pr-2"
				>
					{@render tocList(doc)}
				</nav>
			</aside>

			<div class="min-w-0" bind:this={contentEl}>
				<DocumentView {doc} />

				<!-- Reference adjacency, aligned to the reading measure. -->
				<section
					class="mx-auto mt-12 border-t border-line pt-6"
					style="max-width: var(--spacing-measure)"
				>
					<h2 class="text-lg font-bold text-ink">参照関係</h2>
					<div class="mt-4 grid gap-6 sm:grid-cols-2">
						{@render refGroup('この規則が参照している規則', outgoing, 'out')}
						{@render refGroup('この規則を参照している規則', incoming, 'in')}
					</div>
				</section>
			</div>
		</div>
	{/if}
</div>
