// The single frontend contract, mirroring `crates/core/src/model.rs` (Document,
// serde snake_case) and the CLI's generated `site.json` / `graph.json` (camelCase
// where noted) and `core::search::Hit`. Keep these in lockstep with the Rust
// `#[derive(Serialize)]` shapes — the design spine is that the WASM `parse` output
// and the native `build` output are byte-identical, so one type describes both.

// ── Dates (crates/core/src/dates.rs) ──────────────────────────────────────────

export type Era = 'meiji' | 'taisho' | 'showa' | 'heisei' | 'reiwa'

export interface EraDate {
	era: Era
	year: number
	month: number
	day: number
	/** Gregorian ISO rendering, e.g. "2008-03-26". */
	iso: string
}

export interface EraYear {
	era: Era
	year: number
}

export interface RuleNumber {
	number: number
	year: EraYear | null
	raw: string
}

export interface Enactment {
	date: EraDate | null
	rule_number: RuleNumber | null
	raw: string
}

/** `Effective` — serde internally tagged on `kind`. */
export type Effective =
	| ({ kind: 'date' } & EraDate)
	| { kind: 'on_promulgation' }
	| { kind: 'unspecified' }

// ── Body tree (model.rs) ──────────────────────────────────────────────────────

export interface BranchedNumber {
	main: number
	branch: number | null
}

export interface Subitem {
	label: string
	text: string
}

export interface Item {
	number: number
	text: string
	subitems: Subitem[]
}

export interface Paragraph {
	number: number
	text: string
	items: Item[]
}

export interface Article {
	number: BranchedNumber
	title: string | null
	paragraphs: Paragraph[]
}

/** `BodyNode` — serde internally tagged on `type`. */
export type BodyNode =
	| { type: 'chapter'; number: BranchedNumber; title: string; children: BodyNode[] }
	| { type: 'section'; number: BranchedNumber; title: string; children: BodyNode[] }
	| ({ type: 'article' } & Article)

// ── 附則 / 別表 (model.rs) ─────────────────────────────────────────────────────

export interface SupplProvision {
	ordinal: number
	heading_raw: string
	amendment: RuleNumber | null
	promulgated: EraDate | null
	effective: Effective
	text: string
	references_tables: string[]
}

export type AppendixKind = 'table' | 'style'

export interface Table {
	header_row_count: number
	rows: string[][]
}

export interface Appendix {
	kind: AppendixKind
	id: string
	related_article_raw: string
	raw_text: string
	cells: Table | null
}

export interface Document {
	title: string
	enacted: Enactment
	last_amended: Enactment | null
	full_amendment_note: string | null
	body: BodyNode[]
	supplementary_provisions: SupplProvision[]
	appendices: Appendix[]
}

// ── site.json (CLI, camelCase) ────────────────────────────────────────────────

export interface RulesetEntry {
	code: string
	name: string
	sourceUrl: string
	/** false for the scanned, text-less PDF (230007) — degrades to a PDF-only card. */
	hasData: boolean
	category: string
	articleCount: number
}

export interface Site {
	rulesets: RulesetEntry[]
}

// ── graph.json (CLI) ──────────────────────────────────────────────────────────

export interface GraphNode {
	code: string
	name: string
}

export interface GraphEdge {
	from: string
	to: string
	/** Article labels where the reference occurs, e.g. ["第1条"]. */
	articles: string[]
	count: number
}

export interface Graph {
	nodes: GraphNode[]
	edges: GraphEdge[]
}

// ── search (core::search::Hit, snake_case) ────────────────────────────────────

export interface Hit {
	code: string
	article: string
	title: string | null
	snippet: string
	/** Highlight start, char offset into `snippet`. */
	hl_start: number
	/** Highlight length, in chars. */
	hl_len: number
	score: number
}
