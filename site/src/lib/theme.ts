/**
 * One source of truth for light/dark.
 *
 * There used to be two: the landing page held its own `useState("light")` and
 * the docs layout read the class off `<html>`. Because the landing page's
 * initial value was a hardcoded literal rather than the current theme, its
 * mount effect stripped the `dark` class — so going from any docs page back to
 * the landing page silently reverted to light, and a reload always did.
 *
 * Every consumer now reads and writes the same place, in this order:
 *   1. the visitor's stored choice,
 *   2. their operating system preference,
 *   3. light.
 */

export type Theme = "light" | "dark";

export const THEME_STORAGE_KEY = "glimps-theme";

/** Reading localStorage throws in some privacy modes; never let that break the page. */
function readStoredTheme(): Theme | null {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    return stored === "light" || stored === "dark" ? stored : null;
  } catch {
    return null;
  }
}

function systemTheme(): Theme {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** The theme that should be showing right now. Safe to call during render. */
export function resolveTheme(): Theme {
  if (typeof window === "undefined") return "light";
  return readStoredTheme() ?? systemTheme();
}

/** Paint it and remember it. The class is what every `.dark` rule in styles.css keys off. */
export function applyTheme(theme: Theme) {
  document.documentElement.classList.toggle("dark", theme === "dark");
  document.documentElement.style.colorScheme = theme;
  try {
    window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // A visitor with storage blocked still gets a working toggle for this
    // session; it just will not survive a reload.
  }
}

/**
 * The inline script that runs before first paint, so the correct theme is on
 * `<html>` before React mounts. Without it the page paints light and then
 * flips, which is worse than the bug this replaced.
 *
 * Kept as a string next to the logic above so the two cannot drift apart.
 */
export const THEME_INIT_SCRIPT = `
(function () {
  try {
    var stored = localStorage.getItem('${THEME_STORAGE_KEY}');
    var theme = stored === 'light' || stored === 'dark'
      ? stored
      : (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
    document.documentElement.classList.toggle('dark', theme === 'dark');
    document.documentElement.style.colorScheme = theme;
  } catch (e) {}
})();
`.trim();
