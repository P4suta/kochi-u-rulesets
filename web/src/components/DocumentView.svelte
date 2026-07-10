<script lang="ts">
	// The shared Document → DOM renderer. Used by both the saved-document page and
	// the live-parser view, so the two are visually identical by construction — the
	// design spine ("one contract") carried all the way to the UI.
	import type { Appendix, BodyNode, Document } from '../types'
	import {
		articleLabel,
		branchedLabel,
		effectiveLabel,
		enactmentLabel,
		kanjiNumeral,
		ruleNumberLabel,
	} from '../lib/format'

	let { doc }: { doc: Document } = $props()
</script>

{#snippet bodyNodeView(node: BodyNode)}
	{#if node.type === 'chapter' || node.type === 'section'}
		<section class="mt-8">
			<h2
				class={node.type === 'chapter'
					? 'text-lg font-bold text-ink'
					: 'text-base font-semibold text-ink-2'}
			>
				{branchedLabel(node.number, node.type === 'chapter' ? '章' : '節')}
				<span class="ml-2">{node.title}</span>
			</h2>
			{#each node.children as child, i (i)}
				{@render bodyNodeView(child)}
			{/each}
		</section>
	{:else}
		<article id={articleLabel(node.number)} class="mt-6 scroll-mt-20">
			<h3 class="font-semibold text-ink">
				<span class="text-accent">{articleLabel(node.number)}</span>
				{#if node.title}<span class="ml-1 text-ink">（{node.title}）</span>{/if}
			</h3>
			{#each node.paragraphs as para, i (i)}
				<div class="mt-2">
					<p>
						{#if para.number > 1}<span class="mr-1 text-ink-3 tabular-nums">{para.number}</span
							>{/if}{para.text}
					</p>
					{#if para.items.length > 0}
						<ol class="mt-1 space-y-1">
							{#each para.items as item, j (j)}
								<li class="flex gap-2">
									<span class="shrink-0 text-ink-2">{kanjiNumeral(item.number)}</span>
									<div class="min-w-0">
										<span>{item.text}</span>
										{#if item.subitems.length > 0}
											<ul class="mt-1 space-y-1">
												{#each item.subitems as sub, k (k)}
													<li class="flex gap-2">
														<span class="shrink-0 text-ink-2">{sub.label}</span>
														<span class="min-w-0">{sub.text}</span>
													</li>
												{/each}
											</ul>
										{/if}
									</div>
								</li>
							{/each}
						</ol>
					{/if}
				</div>
			{/each}
		</article>
	{/if}
{/snippet}

{#snippet appendixView(ap: Appendix)}
	<section class="mt-8">
		<h3 class="font-semibold text-ink">
			{ap.id}
			{#if ap.related_article_raw}<span class="ml-2 text-sm text-ink-3">{ap.related_article_raw}</span
				>{/if}
		</h3>
		{#if ap.cells}
			<div class="mt-2 overflow-x-auto rounded-lg border border-line">
				<table class="w-full border-collapse text-sm">
					<tbody>
						{#each ap.cells.rows as row, r (r)}
							<tr class="border-t border-line-soft first:border-t-0">
								{#each row as cell, c (c)}
									{#if r < ap.cells.header_row_count}
										<th class="border-r border-line-soft bg-fill px-3 py-1.5 text-left font-semibold last:border-r-0"
											>{cell}</th
										>
									{:else}
										<td class="border-r border-line-soft px-3 py-1.5 align-top last:border-r-0">{cell}</td>
									{/if}
								{/each}
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{:else}
			<pre
				class="mt-2 overflow-x-auto whitespace-pre-wrap rounded-lg border border-line bg-fill px-3 py-2 font-serif text-sm text-ink-2">{ap.raw_text}</pre>
		{/if}
	</section>
{/snippet}

<div class="prose-legal mx-auto">
	<header class="border-b border-line pb-4">
		<h1 class="text-2xl font-bold tracking-tight text-ink">{doc.title}</h1>
		<dl class="mt-2 space-y-0.5 text-sm text-ink-2">
			<div><dt class="inline text-ink-3">制定：</dt> <dd class="inline">{enactmentLabel(doc.enacted)}</dd></div>
			{#if doc.last_amended}
				<div><dt class="inline text-ink-3">最終改正：</dt> <dd class="inline">{enactmentLabel(doc.last_amended)}</dd></div>
			{/if}
			{#if doc.full_amendment_note}
				<div class="text-ink-3">{doc.full_amendment_note}</div>
			{/if}
		</dl>
	</header>

	{#each doc.body as node, i (i)}
		{@render bodyNodeView(node)}
	{/each}

	{#if doc.appendices.length > 0}
		<div class="mt-12 border-t border-line pt-2">
			{#each doc.appendices as ap, i (i)}
				{@render appendixView(ap)}
			{/each}
		</div>
	{/if}

	{#if doc.supplementary_provisions.length > 0}
		<div class="mt-12 border-t border-line pt-4">
			<h2 class="text-lg font-bold text-ink">附則</h2>
			{#each doc.supplementary_provisions as sp, i (i)}
				<section class="mt-4">
					<h3 class="text-sm font-semibold text-ink-2">
						{sp.heading_raw}
						{#if sp.amendment}<span class="ml-1 text-ink-3">（{ruleNumberLabel(sp.amendment)}）</span>{/if}
					</h3>
					<p class="mt-1 text-xs text-ink-3">{effectiveLabel(sp.effective)}</p>
					{#if sp.text}<p class="mt-1 text-sm">{sp.text}</p>{/if}
				</section>
			{/each}
		</div>
	{/if}
</div>
