<script lang="ts">
	// The hub panel: browse the rulesets, or — the moment the shared search box has
	// a query — the same panel becomes ranked full-text results. List + Search, one
	// surface. Search state lives in the `search` store (so it survives tab
	// switches); this component only renders it.
	import type { Hit, RulesetEntry, Site } from '../types'
	import { search } from '../lib/search.svelte'
	import { href } from '../lib/router.svelte'

	let { site }: { site: Site } = $props()

	const searching = $derived(search.query.trim() !== '')

	// ── Browse (empty query) ─────────────────────────────────────────────────────
	let active = $state<string>('すべて')

	const categories = $derived.by(() => {
		const counts = new Map<string, number>()
		for (const r of site.rulesets) counts.set(r.category, (counts.get(r.category) ?? 0) + 1)
		const sorted = [...counts.entries()].sort(
			(a, b) => b[1] - a[1] || a[0].localeCompare(b[0], 'ja'),
		)
		return ['すべて', ...sorted.map(([c]) => c)]
	})

	const filtered = $derived(
		active === 'すべて' ? site.rulesets : site.rulesets.filter((r) => r.category === active),
	)

	function count(cat: string): number {
		return cat === 'すべて'
			? site.rulesets.length
			: site.rulesets.filter((r) => r.category === cat).length
	}

	// ── Search (query present) ───────────────────────────────────────────────────
	const nameByCode = $derived.by(() => {
		const m = new Map<string, string>()
		for (const r of site.rulesets) m.set(r.code, r.name)
		return m
	})

	const groups = $derived.by(() => {
		const map = new Map<string, Hit[]>()
		for (const h of search.results) {
			const arr = map.get(h.code)
			if (arr) arr.push(h)
			else map.set(h.code, [h])
		}
		return [...map.entries()].map(([code, hits]) => ({
			code,
			name: nameByCode.get(code) ?? code,
			hits,
		}))
	})

	/** Split a snippet at CHARACTER (code point) offsets and expose the matched span. */
	function highlight(snippet: string, hlStart: number, hlLen: number) {
		const chars = Array.from(snippet)
		const s = Math.max(0, Math.min(hlStart, chars.length))
		const e = Math.max(s, Math.min(hlStart + hlLen, chars.length))
		return {
			before: chars.slice(0, s).join(''),
			match: chars.slice(s, e).join(''),
			after: chars.slice(e).join(''),
		}
	}
</script>

{#snippet card(r: RulesetEntry)}
	{#if r.hasData}
		<a
			href={href({ name: 'doc', code: r.code })}
			class="block h-full rounded-card border border-line bg-card p-4 shadow-card transition-transform hover:-translate-y-0.5"
		>
			<div class="flex items-center gap-2 text-xs text-ink-3">
				<span class="rounded bg-fill px-1.5 py-0.5">{r.category}</span>
				<span class="tabular-nums">全{r.articleCount}条</span>
			</div>
			<h2 class="mt-2 font-semibold leading-snug text-ink">{r.name}</h2>
			<p class="mt-1 font-mono text-xs text-ink-3">{r.code}</p>
		</a>
	{:else}
		<div class="flex h-full flex-col rounded-card border border-dashed border-line bg-fill/40 p-4">
			<div class="flex items-center gap-2 text-xs text-ink-3">
				<span class="rounded bg-fill px-1.5 py-0.5">{r.category}</span>
				<span>スキャンPDF（本文データなし）</span>
			</div>
			<h2 class="mt-2 font-semibold leading-snug text-ink-2">{r.name}</h2>
			<a
				href={r.sourceUrl}
				target="_blank"
				rel="noopener"
				class="mt-auto pt-2 text-sm text-accent hover:underline"
			>
				原典PDFを開く →
			</a>
		</div>
	{/if}
{/snippet}

<section>
	{#if !searching}
		<!-- ── 一覧（検索語なし） ── -->
		<div class="flex flex-wrap items-baseline justify-between gap-2">
			<h1 class="text-xl font-bold text-ink">規則一覧</h1>
			<p class="text-sm text-ink-3">
				{#if search.docCount != null}<span class="tabular-nums">{search.docCount}</span>規則を全文検索できます{:else}全{site.rulesets.length}件{/if}
			</p>
		</div>

		<div class="mt-4 flex flex-wrap gap-2">
			{#each categories as cat (cat)}
				<button
					type="button"
					onclick={() => (active = cat)}
					class="rounded-full border px-3 py-1 text-sm transition-colors {active === cat
						? 'border-accent bg-accent text-on-accent'
						: 'border-line text-ink-2 hover:bg-fill'}"
				>
					{cat}<span class="ml-1.5 tabular-nums opacity-70">{count(cat)}</span>
				</button>
			{/each}
		</div>

		<ul class="mt-5 grid gap-3 sm:grid-cols-2">
			{#each filtered as r (r.code)}
				<li>{@render card(r)}</li>
			{/each}
		</ul>
	{:else if search.error}
		<!-- ── 検索：エラー ── -->
		<p class="mt-4 text-center text-sm text-ink-3">{search.error}</p>
	{:else if search.loading}
		<!-- ── 検索：読み込み ── -->
		<div class="mt-6 flex items-center justify-center gap-2 text-sm text-ink-3">
			<span
				class="inline-block size-4 animate-spin rounded-full border-2 border-line border-t-accent"
				aria-hidden="true"
			></span>
			検索中…
		</div>
	{:else if search.results.length === 0}
		<!-- ── 検索：ヒットなし ── -->
		<p class="mt-6 text-center text-sm text-ink-3">
			「{search.query}」に該当する条文がありません
		</p>
	{:else}
		<!-- ── 検索：結果 ── -->
		<p class="text-sm text-ink-3">
			「{search.query}」の検索結果：<span class="tabular-nums">{search.results.length}</span>件の条文（<span
				class="tabular-nums">{groups.length}</span
			>規則）
		</p>
		<div class="mt-3 space-y-5">
			{#each groups as group (group.code)}
				<div class="rounded-card border border-line bg-card shadow-card">
					<a
						href={href({ name: 'doc', code: group.code })}
						class="flex items-baseline gap-2 border-b border-line-soft px-4 py-2.5 transition-colors hover:bg-fill"
					>
						<span class="min-w-0 flex-1 font-semibold leading-snug text-ink">{group.name}</span>
						<span class="shrink-0 font-mono text-xs text-ink-3">{group.code}</span>
					</a>
					<ul class="divide-y divide-line-soft">
						{#each group.hits as hit (hit.article + ' ' + hit.snippet)}
							{@const parts = highlight(hit.snippet, hit.hl_start, hit.hl_len)}
							<li>
								<a
									href={href({ name: 'doc', code: hit.code, article: hit.article })}
									class="block px-4 py-2.5 transition-colors hover:bg-fill"
								>
									<div class="flex items-baseline gap-2">
										<span class="shrink-0 text-sm font-medium text-accent">{hit.article}</span>
										{#if hit.title}<span class="min-w-0 truncate text-sm text-ink-2">（{hit.title}）</span
											>{/if}
									</div>
									<p class="mt-1 text-sm leading-relaxed text-ink-2">
										{parts.before}{#if parts.match}<mark>{parts.match}</mark>{/if}{parts.after}
									</p>
								</a>
							</li>
						{/each}
					</ul>
				</div>
			{/each}
		</div>
	{/if}
</section>
