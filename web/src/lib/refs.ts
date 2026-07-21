// Turns a body text + its inline `TextRef`s into renderable pieces, and resolves
// each ref to a navigation target. Offsets are character (Unicode scalar) indices,
// so slicing goes through `Array.from(text)` — `text.slice` would cut CJK wrong.

import type { TextRef } from '../types'
import { href } from './router.svelte'
import { articleLabel, branchedLabel } from './format'

export interface Segment {
	text: string
	ref: TextRef | null
}

/** Splits `text` into plain and referenced pieces, in order. Overlapping or
 *  out-of-bounds refs are skipped defensively (the pass emits non-overlapping refs). */
export function segments(text: string, refs: TextRef[]): Segment[] {
	if (!refs.length) return [{ text, ref: null }]
	const chars = Array.from(text)
	const sorted = [...refs].sort((a, b) => a.start - b.start)
	const out: Segment[] = []
	let pos = 0
	for (const ref of sorted) {
		if (ref.start < pos || ref.end > chars.length || ref.start >= ref.end) continue
		if (ref.start > pos) out.push({ text: chars.slice(pos, ref.start).join(''), ref: null })
		out.push({ text: chars.slice(ref.start, ref.end).join(''), ref })
		pos = ref.end
	}
	if (pos < chars.length) out.push({ text: chars.slice(pos).join(''), ref: null })
	return out
}

/** The hash a ref navigates to, or `null` when it is a marker with no single target
 *  (an out-of-corpus 別に定める, or one with several candidate rules → use the panel).
 *  `code` is the current document; `article` its label, needed for same-article 項 refs. */
export function refHref(ref: TextRef, code: string, article: string): string | null {
	switch (ref.kind) {
		case 'paragraph':
			return href({ name: 'doc', code, article })
		case 'article':
			return href({ name: 'doc', code, article: articleLabel(ref.article) })
		case 'table':
			return href({ name: 'doc', code, article: ref.appendix_id })
		case 'rule':
			return href({ name: 'doc', code: ref.code })
		case 'separately_provided':
			return ref.rules.length === 1 ? href({ name: 'doc', code: ref.rules[0] }) : null
	}
}

/** Which visual register a ref belongs to: `in` = jump within this document (subtle),
 *  `out` = another ruleset (accent), `mark` = a 別に定める with no single link target. */
export function refKindClass(ref: TextRef): 'in' | 'out' | 'mark' {
	switch (ref.kind) {
		case 'paragraph':
		case 'article':
		case 'table':
			return 'in'
		case 'rule':
			return 'out'
		case 'separately_provided':
			return ref.rules.length === 1 ? 'out' : 'mark'
	}
}

/** Hover text for a ref, mainly to explain a 別に定める marker. `nameOf` maps a code
 *  to its display name. */
export function refTitle(ref: TextRef, nameOf: (code: string) => string): string | undefined {
	switch (ref.kind) {
		case 'separately_provided':
			if (ref.rules.length === 0) return '委任規定（別に定める）— 集内に対応する規則なし'
			if (ref.rules.length === 1) return nameOf(ref.rules[0])
			return `別に定める規則: ${ref.rules.map(nameOf).join('、')}`
		case 'rule':
			return nameOf(ref.code)
		case 'article':
			return ref.paragraph != null
				? `${branchedLabel(ref.article, '条')}第${ref.paragraph}項`
				: undefined
		default:
			return undefined
	}
}
