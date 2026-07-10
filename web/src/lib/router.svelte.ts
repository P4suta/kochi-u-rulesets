// A dependency-free hash router. Permalinks are keyed by the stable ruleset
// `code` (never the display name), so a rule rename never breaks a link:
//   #/            → the ruleset list (home)
//   #/210001      → a document
//   #/210001/第5条 → a document scrolled to an article (the anchor is the 条 label)
//   #/search?q=…  → full-text search
//   #/graph       → the reference graph
//   #/timeline    → the site-wide amendment timeline
//   #/parser      → the drag-and-drop live PDF parser
// Numeric codes never collide with the reserved view keywords below.

export type RouteName = 'list' | 'doc' | 'search' | 'graph' | 'timeline' | 'parser'

export interface Route {
	name: RouteName
	/** Ruleset code, for `doc`. */
	code?: string
	/** Article label anchor (e.g. "第5条"), for `doc`. */
	article?: string
	/** Query string, for `search`. */
	query?: string
}

const KEYWORDS = new Set<RouteName>(['search', 'graph', 'timeline', 'parser'])

/** Parse `location.hash` into a `Route`. Unknown shapes fall back to the list. */
export function parseHash(hash: string): Route {
	const raw = hash.replace(/^#\/?/, '')
	if (raw === '') return { name: 'list' }

	const [pathPart, queryPart] = raw.split('?')
	const segments = pathPart.split('/').filter(Boolean).map(decodeURIComponent)
	const head = segments[0]

	if (head && KEYWORDS.has(head as RouteName)) {
		const params = new URLSearchParams(queryPart ?? '')
		return { name: head as RouteName, query: params.get('q') ?? undefined }
	}
	if (head) {
		return { name: 'doc', code: head, article: segments[1] }
	}
	return { name: 'list' }
}

/** Build a hash for programmatic navigation (mirrors `parseHash`). */
export function href(route: Route): string {
	switch (route.name) {
		case 'list':
			return '#/'
		case 'doc':
			return route.article
				? `#/${route.code}/${encodeURIComponent(route.article)}`
				: `#/${route.code}`
		case 'search':
			return route.query ? `#/search?q=${encodeURIComponent(route.query)}` : '#/search'
		default:
			return `#/${route.name}`
	}
}

/** Reactive router singleton. Components read `router.route`; navigate by setting
 *  `location.hash` (via `href`) or calling `navigate`. */
class Router {
	route = $state<Route>(parseHash(location.hash))

	constructor() {
		window.addEventListener('hashchange', () => {
			this.route = parseHash(location.hash)
		})
	}

	navigate(route: Route): void {
		location.hash = href(route)
	}
}

export const router = new Router()
