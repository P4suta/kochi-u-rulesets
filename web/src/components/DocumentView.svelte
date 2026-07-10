<script lang="ts">
	// The shared Document → DOM renderer. Body prose is rendered through the inline
	// reference layer (別に定める / 前項 / 第N条 / 別表第N / other rulesets become
	// links), a 条 carries a "別に定める規則" panel from the reverse 制定根拠 index,
	// and the header shows this ruleset's own 根拠 (upward links).
	import type { Appendix, Authority, BodyNode, Document, TextRef } from '../types'
	import {
		articleLabel,
		branchedLabel,
		effectiveLabel,
		enactmentLabel,
		kanjiNumeral,
		ruleNumberLabel,
	} from '../lib/format'
	import { href } from '../lib/router.svelte'
	import { refHref, refKindClass, refTitle, segments } from '../lib/refs'

	let {
		doc,
		code,
		nameOf,
	}: { doc: Document; code: string; nameOf: (code: string) => string } = $props()

	// Visual register per reference class (see refKindClass). These literals are kept
	// whole so Tailwind's scanner sees them.
	const REF_CLASS = {
		in: 'text-ink underline decoration-dotted decoration-line underline-offset-2 hover:text-accent hover:decoration-accent',
		out: 'text-accent hover:underline',
		mark: 'text-ink-2 underline decoration-dotted decoration-ink-3 underline-offset-2 cursor-help',
	}

	// "第X条第Y項" for an authority citation (either part may be absent).
	function clauseLabel(a: Authority): string {
		const art = a.article ? branchedLabel(a.article, '条') : ''
		const par = a.paragraph != null ? `第${a.paragraph}項` : ''
		return art + par
	}
	function authorityHref(a: Authority): string {
		if (!a.rule_code) return '#'
		return a.article
			? href({ name: 'doc', code: a.rule_code, article: branchedLabel(a.article, '条') })
			: href({ name: 'doc', code: a.rule_code })
	}
</script>

<!-- Body prose with inline references resolved to links/markers. -->
{#snippet richText(text: string, refs: TextRef[], artLabel: string)}
	{#each segments(text, refs) as seg, i (i)}
		{#if seg.ref}
			{@const h = refHref(seg.ref, code, artLabel)}
			{#if h}
				<a href={h} class={REF_CLASS[refKindClass(seg.ref)]} title={refTitle(seg.ref, nameOf)}
					>{seg.text}</a
				>
			{:else}
				<span class={REF_CLASS.mark} title={refTitle(seg.ref, nameOf)}>{seg.text}</span>
			{/if}
		{:else}{seg.text}{/if}
	{/each}
{/snippet}

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
		{@const artLabel = articleLabel(node.number)}
		<article id={artLabel} class="mt-6 scroll-mt-20">
			<h3 class="font-semibold text-ink">
				<span class="text-accent">{artLabel}</span>
				{#if node.title}<span class="ml-1 text-ink">（{node.title}）</span>{/if}
			</h3>
			{#each node.paragraphs as para, i (i)}
				<div class="mt-2">
					<p>
						{#if para.number > 1}<span class="mr-1 text-ink-3 tabular-nums">{para.number}</span
							>{/if}{@render richText(para.text, para.refs, artLabel)}
					</p>
					{#if para.items.length > 0}
						<ol class="mt-1 space-y-1">
							{#each para.items as item, j (j)}
								<li class="flex gap-2">
									<span class="shrink-0 text-ink-2">{kanjiNumeral(item.number)}</span>
									<div class="min-w-0">
										<span>{@render richText(item.text, item.refs, artLabel)}</span>
										{#if item.subitems.length > 0}
											<ul class="mt-1 space-y-1">
												{#each item.subitems as sub, k (k)}
													<li class="flex gap-2">
														<span class="shrink-0 text-ink-2">{sub.label}</span>
														<span class="min-w-0"
															>{@render richText(sub.text, sub.refs, artLabel)}</span
														>
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

			{#if node.subordinate_rules.length > 0}
				<div class="mt-3 rounded-md border border-line-soft bg-fill px-3 py-2 text-sm">
					<span class="text-ink-3">▸ この条で「別に定める」規則：</span>
					{#each node.subordinate_rules as rc, i (rc)}
						{#if i > 0}<span class="text-ink-3">、</span>{/if}<a
							href={href({ name: 'doc', code: rc })}
							class="font-medium text-accent hover:underline">{nameOf(rc)}</a
						>
					{/each}
				</div>
			{/if}
		</article>
	{/if}
{/snippet}

{#snippet appendixView(ap: Appendix)}
	<section id={ap.id} class="mt-8 scroll-mt-20">
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

<div class="prose-legal reading mx-auto">
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
			{#if doc.authorities.length > 0}
				<div>
					<dt class="inline text-ink-3">根拠：</dt>
					<dd class="inline">
						{#each doc.authorities as a, i (i)}
							{#if i > 0}<span class="text-ink-3">　</span>{/if}{#if a.rule_code}<a
									href={authorityHref(a)}
									class="text-accent hover:underline"
									>{nameOf(a.rule_code)}{clauseLabel(a)}</a
								>{:else}<span>{a.rule_name}{clauseLabel(a)}</span>{/if}
						{/each}
					</dd>
				</div>
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

<style>
	/* Reading focus: hovering a 条 keeps it bright and fades the other articles, so
	   the eye settles on one provision at a time. Only <article> leaves are dimmed
	   (章/節 headings and 別表/附則 stay as landmarks) — dimming a wrapper would
	   multiply opacity down onto the hovered article. Pure CSS, no JS. */
	.reading :global(article) {
		transition: opacity 0.25s ease;
	}
	.reading:has(:global(article:hover)) :global(article) {
		opacity: 0.4;
	}
	.reading:has(:global(article:hover)) :global(article:hover) {
		opacity: 1;
	}
</style>
