<script lang="ts">
	import type { RulesetEntry, Site } from '../types'
	import { href } from '../lib/router.svelte'

	let { site }: { site: Site } = $props()

	// Category facet, derived from the data (order: by frequency, then name) plus a
	// leading "すべて". Filtering is a plain in-memory predicate over site.json.
	let active = $state<string>('すべて')

	const categories = $derived.by(() => {
		const counts = new Map<string, number>()
		for (const r of site.rulesets) counts.set(r.category, (counts.get(r.category) ?? 0) + 1)
		const sorted = [...counts.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0], 'ja'))
		return ['すべて', ...sorted.map(([c]) => c)]
	})

	const filtered = $derived(
		active === 'すべて'
			? site.rulesets
			: site.rulesets.filter((r) => r.category === active),
	)

	function count(cat: string): number {
		return cat === 'すべて' ? site.rulesets.length : site.rulesets.filter((r) => r.category === cat).length
	}
</script>

<section>
	<h1 class="text-xl font-bold text-ink">規則一覧</h1>
	<p class="mt-1 text-sm text-ink-3">
		{site.rulesets.length}件の規則。カテゴリで絞り込み、カードから各規則を開けます。
	</p>

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
			<li>
				{@render card(r)}
			</li>
		{/each}
	</ul>
</section>

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
