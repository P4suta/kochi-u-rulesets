<script lang="ts">
	import type { Document, EraDate, Site } from '../types'
	import { loadDoc } from '../lib/data'
	import { href } from '../lib/router.svelte'
	import { eraDateLabel, ruleNumberLabel, supplSortKey } from '../lib/format'

	let { site }: { site: Site } = $props()

	// A dated point in the corpus-wide chronology. `date` is kept when available so
	// we can render the wareki (和暦) label; otherwise we fall back to the ISO date.
	interface TimelineEvent {
		iso: string
		date: EraDate | null
		kind: '制定' | '改正'
		code: string
		name: string
		ruleNumber: string | null
	}

	// Only rulesets with structured data can contribute events.
	const dataRulesets = $derived(site.rulesets.filter((r) => r.hasData))

	// Documents load once (loadDoc is cached). Null = still loading.
	let docs = $state<Map<string, Document> | null>(null)
	let error = $state<string | null>(null)

	$effect(() => {
		let cancelled = false
		docs = null
		error = null
		Promise.all(
			dataRulesets.map((r) => loadDoc(r.code).then((d) => [r.code, d] as const)),
		)
			.then((entries) => {
				if (!cancelled) docs = new Map(entries)
			})
			.catch((e) => {
				if (!cancelled) error = e instanceof Error ? e.message : String(e)
			})
		return () => {
			cancelled = true
		}
	})

	// Flatten every document into dated events: the enactment (制定) plus each 附則
	// (改正). Undated blocks are skipped. Sorted newest-first (ISO sorts lexically).
	const events = $derived.by<TimelineEvent[]>(() => {
		if (!docs) return []
		const nameByCode = new Map(site.rulesets.map((r) => [r.code, r.name]))
		const out: TimelineEvent[] = []
		for (const [code, doc] of docs) {
			const name = nameByCode.get(code) ?? doc.title
			if (doc.enacted.date) {
				out.push({
					iso: doc.enacted.date.iso,
					date: doc.enacted.date,
					kind: '制定',
					code,
					name,
					ruleNumber: doc.enacted.rule_number ? ruleNumberLabel(doc.enacted.rule_number) : null,
				})
			}
			for (const sp of doc.supplementary_provisions) {
				const iso = supplSortKey(sp.promulgated, sp.effective)
				if (!iso) continue
				out.push({
					iso,
					date: sp.promulgated,
					kind: '改正',
					code,
					name,
					ruleNumber: sp.amendment ? ruleNumberLabel(sp.amendment) : null,
				})
			}
		}
		out.sort((a, b) => (a.iso < b.iso ? 1 : a.iso > b.iso ? -1 : a.code.localeCompare(b.code)))
		return out
	})

	// Group the sorted events into contiguous runs by ISO year.
	const grouped = $derived.by<{ year: string; events: TimelineEvent[] }[]>(() => {
		const groups: { year: string; events: TimelineEvent[] }[] = []
		let current: { year: string; events: TimelineEvent[] } | null = null
		for (const e of events) {
			const year = e.iso.slice(0, 4)
			if (!current || current.year !== year) {
				current = { year, events: [] }
				groups.push(current)
			}
			current.events.push(e)
		}
		return groups
	})
</script>

<section>
	<h1 class="text-xl font-bold text-ink">改正の年表</h1>
	<p class="mt-1 text-sm text-ink-3">
		全規則の制定・改正を日付順に並べた年表です。新しいものから表示し、各項目から該当の規則を開けます。
	</p>

	{#if error}
		<div class="mt-6 rounded-card border border-line bg-card p-4 text-sm text-ink-2 shadow-card">
			<p class="font-semibold text-ink">読み込みに失敗しました</p>
			<p class="mt-1 text-ink-3">{error}</p>
		</div>
	{:else if docs === null}
		<p class="mt-6 text-sm text-ink-3">読み込み中…</p>
	{:else if events.length === 0}
		<p class="mt-6 text-sm text-ink-3">表示できる日付付きの記録がありません。</p>
	{:else}
		<p class="mt-4 text-xs text-ink-3 tabular-nums">全{events.length}件</p>

		<div class="mt-4 space-y-8">
			{#each grouped as group (group.year)}
				<div>
					<h2 class="flex items-baseline gap-2 border-b border-line pb-1">
						<span class="text-lg font-bold text-ink tabular-nums">{group.year}</span>
						<span class="text-xs text-ink-3 tabular-nums">{group.events.length}件</span>
					</h2>

					<ul class="mt-3 border-l border-line-soft pl-4">
						{#each group.events as e, i (`${e.code}-${e.iso}-${e.kind}-${i}`)}
							<li class="relative py-2.5">
								<span
									class="absolute -left-[1.3125rem] top-4 h-2 w-2 rounded-full border border-paper {e.kind ===
									'制定'
										? 'bg-accent'
										: 'bg-fill-strong'}"
									aria-hidden="true"
								></span>
								<div class="flex flex-wrap items-baseline gap-x-2 gap-y-1">
									<span class="w-32 shrink-0 text-sm text-ink-2 tabular-nums">
										{e.date ? eraDateLabel(e.date) : e.iso}
									</span>
									<span
										class="rounded px-1.5 py-0.5 text-xs {e.kind === '制定'
											? 'bg-accent text-on-accent'
											: 'bg-fill text-ink-2'}"
									>
										{e.kind}
									</span>
									<a
										href={href({ name: 'doc', code: e.code })}
										class="font-medium text-ink hover:text-accent hover:underline"
									>
										{e.name}
									</a>
									{#if e.ruleNumber}
										<span class="text-xs text-ink-3">{e.ruleNumber}</span>
									{/if}
								</div>
							</li>
						{/each}
					</ul>
				</div>
			{/each}
		</div>
	{/if}
</section>
