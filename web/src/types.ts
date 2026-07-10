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

/** `RefTarget` — serde internally tagged on `kind`, flattened into `TextRef`. */
export type RefTarget =
	| { kind: 'paragraph'; paragraph: number }
	| { kind: 'article'; article: BranchedNumber; paragraph: number | null }
	| { kind: 'table'; appendix_id: string }
	| { kind: 'rule'; code: string }
	| { kind: 'separately_provided'; rules: string[] }

/** An inline reference span. `start`/`end` are **character** offsets into the sibling
 *  `text` (Unicode scalars) — slice with `Array.from(text)`, never `text.slice`. */
export type TextRef = { start: number; end: number } & RefTarget

export interface Subitem {
	label: string
	text: string
	refs: TextRef[]
}

export interface Item {
	number: number
	text: string
	subitems: Subitem[]
	refs: TextRef[]
}

export interface Paragraph {
	number: number
	text: string
	items: Item[]
	refs: TextRef[]
}

export interface Article {
	number: BranchedNumber
	title: string | null
	paragraphs: Paragraph[]
	/** Child rulesets (codes) enacted under this 条 — the reverse of 制定根拠. */
	subordinate_rules: string[]
}

/** `Authority` — one 制定根拠 citation ("親規則名第X条第Y項…の規定に基づき"). */
export interface Authority {
	rule_code: string | null
	rule_name: string
	article: BranchedNumber | null
	paragraph: number | null
	raw: string
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
	authorities: Authority[]
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

/** `authority` = 制定根拠 (from delegates to → to, the 別に定める hierarchy);
 *  `reference` = a plain by-name mention (from cites to). */
export type EdgeKind = 'authority' | 'reference'

export interface GraphEdge {
	from: string
	to: string
	kind: EdgeKind
	/** For authority: the parent 条 the delegation sits in; for reference: the citing
	 *  条 in `from`. e.g. ["第21条"]. */
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
