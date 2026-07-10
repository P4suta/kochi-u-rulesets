// A dependency-free hash router. Everything lives on one page (`home`): browse +
// search, the amendment timeline, and the reference graph are stacked sections
// there, reached by in-page scrolling. The only other screen is a document
// drill-in, permalinked by the stable ruleset `code` (never the display name), so
// a rule rename never breaks a link:
//   #/            → the one page (browse + search + 沿革 + 参照)
//   #/?q=…        → the one page with a full-text query active
//   #/210001      → a document
//   #/210001/第5条 → a document scrolled to an article (anchor = the 条 label)

export type RouteName = 'home' | 'doc'

export interface Route {
	name: RouteName
	/** Ruleset code, for `doc`. */
	code?: string
	/** Article label anchor (e.g. "第5条"), for `doc`. */
	article?: string
	/** Query string, for `home`. */
	query?: string
}

/** Parse `location.hash` into a `Route`. Unknown shapes fall back to the page. */
export function parseHash(hash: string): Route {
	const raw = hash.replace(/^#\/?/, '')
	const [pathPart, queryPart] = raw.split('?')
	const query = new URLSearchParams(queryPart ?? '').get('q') ?? undefined
	const segments = pathPart.split('/').filter(Boolean).map(decodeURIComponent)
	const head = segments[0]

	if (head) return { name: 'doc', code: head, article: segments[1] }
	return { name: 'home', query }
}

/** Build a hash for programmatic navigation (mirrors `parseHash`). */
export function href(route: Route): string {
	switch (route.name) {
		case 'home':
			return route.query ? `#/?q=${encodeURIComponent(route.query)}` : '#/'
		case 'doc':
			return route.article
				? `#/${route.code}/${encodeURIComponent(route.article)}`
				: `#/${route.code}`
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
