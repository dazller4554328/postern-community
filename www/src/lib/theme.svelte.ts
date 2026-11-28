import { browser } from '$app/environment';

type Theme = 'dark' | 'light';

function readInitial(): Theme {
  if (!browser) return 'dark';
  const attr = document.documentElement.getAttribute('data-theme');
  return attr === 'light' ? 'light' : 'dark';
}

class ThemeStore {
  current = $state<Theme>(readInitial());

  toggle() {
    this.set(this.current === 'dark' ? 'light' : 'dark');
  }

  set(theme: Theme) {
    this.current = theme;
    if (browser) {
      document.documentElement.setAttribute('data-theme', theme);
      try {
        localStorage.setItem('postern-theme', theme);
      } catch {
        /* storage unavailable — in-memory only */
      }
    }
  }
}

export const theme = new ThemeStore();
