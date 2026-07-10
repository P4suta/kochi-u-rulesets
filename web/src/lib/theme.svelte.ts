// Theme store. `auto` follows the OS (no data-theme attribute → the CSS media
// query decides); `light`/`dark` stamp :root[data-theme] to force one way. The
// choice is persisted so it survives reloads.

export type Theme = 'auto' | 'light' | 'dark'

function apply(t: Theme): void {
	const root = document.documentElement
	if (t === 'auto') root.removeAttribute('data-theme')
	else root.setAttribute('data-theme', t)
}

class ThemeStore {
	value = $state<Theme>('auto')

	constructor() {
		const saved = localStorage.getItem('theme')
		if (saved === 'light' || saved === 'dark' || saved === 'auto') this.value = saved
		apply(this.value)
	}

	set(t: Theme): void {
		this.value = t
		localStorage.setItem('theme', t)
		apply(t)
	}

	/** auto → light → dark → auto. */
	cycle(): void {
		this.set(this.value === 'auto' ? 'light' : this.value === 'light' ? 'dark' : 'auto')
	}
}

export const theme = new ThemeStore()
