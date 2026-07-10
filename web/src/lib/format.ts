import type { BranchedNumber, Effective, Enactment, EraDate, Era, RuleNumber } from '../types'

const ERA_KANJI: Record<Era, string> = {
	meiji: '明治',
	taisho: '大正',
	showa: '昭和',
	heisei: '平成',
	reiwa: '令和',
}

/** "第5条" / "第5条の2" (or any unit char: 章/節/条). */
export function branchedLabel(n: BranchedNumber, unit: string): string {
	const base = `第${n.main}${unit}`
	return n.branch != null ? `${base}の${n.branch}` : base
}

export const articleLabel = (n: BranchedNumber): string => branchedLabel(n, '条')

/** "平成20年3月26日" (year 1 renders as 元年). */
export function eraDateLabel(d: EraDate): string {
	const year = d.year === 1 ? '元' : String(d.year)
	return `${ERA_KANJI[d.era]}${year}年${d.month}月${d.day}日`
}

/** "規則第74号" (with an era-year prefix when the citation carries one). */
export function ruleNumberLabel(r: RuleNumber): string {
	const prefix =
		r.year != null ? `${ERA_KANJI[r.year.era]}${r.year.year === 1 ? '元' : r.year.year}年` : ''
	return `${prefix}規則第${r.number}号`
}

/** A one-line enactment/amendment summary, falling back to the raw text. */
export function enactmentLabel(e: Enactment): string {
	const parts: string[] = []
	if (e.date) parts.push(eraDateLabel(e.date))
	if (e.rule_number) parts.push(ruleNumberLabel(e.rule_number))
	return parts.length > 0 ? parts.join('　') : e.raw
}

/** Human-readable 施行 clause. */
export function effectiveLabel(e: Effective): string {
	switch (e.kind) {
		case 'date':
			return `${eraDateLabel(e)}施行`
		case 'on_promulgation':
			return '公布の日から施行'
		case 'unspecified':
			return '施行日不明'
	}
}

const KANJI_DIGITS = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九']

/** Render 1..=99 as a 号-style kanji numeral ("一", "十", "二十一"). Beyond 99 it
 *  falls back to the plain number — no 号 in this corpus reaches that. */
export function kanjiNumeral(n: number): string {
	if (n <= 0 || n >= 100) return String(n)
	if (n < 10) return KANJI_DIGITS[n]
	const tens = Math.floor(n / 10)
	const ones = n % 10
	const tensPart = tens === 1 ? '十' : `${KANJI_DIGITS[tens]}十`
	return ones === 0 ? tensPart : `${tensPart}${KANJI_DIGITS[ones]}`
}

/** The best ISO date for chronological sorting of a 附則 block, or null. */
export function supplSortKey(promulgated: EraDate | null, effective: Effective): string | null {
	if (promulgated) return promulgated.iso
	if (effective.kind === 'date') return effective.iso
	return null
}
