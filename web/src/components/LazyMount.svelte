<script lang="ts">
	// Render children only once they scroll near the viewport, so heavy sections
	// (the timeline fetches every document; the graph runs a layout) don't run on
	// the initial page load. Reserves `minHeight` until shown so the scrollbar and
	// anchor offsets stay stable.
	import type { Snippet } from 'svelte'

	let { children, minHeight = '360px' }: { children: Snippet; minHeight?: string } = $props()

	let el = $state<HTMLElement>()
	let shown = $state(false)

	$effect(() => {
		if (!el || shown) return
		const obs = new IntersectionObserver(
			(entries) => {
				if (entries.some((e) => e.isIntersecting)) {
					shown = true
					obs.disconnect()
				}
			},
			{ rootMargin: '300px 0px' },
		)
		obs.observe(el)
		return () => obs.disconnect()
	})
</script>

<div bind:this={el} style:min-height={shown ? undefined : minHeight}>
	{#if shown}
		{@render children()}
	{:else}
		<p class="py-16 text-center text-sm text-ink-3">読み込み中…</p>
	{/if}
</div>
