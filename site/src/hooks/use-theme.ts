import { useCallback, useEffect, useState } from "react";

import { applyTheme, resolveTheme, THEME_STORAGE_KEY, type Theme } from "@/lib/theme";

/**
 * Read and set the site theme.
 *
 * The initial value is resolved from what is actually stored, not a hardcoded
 * literal — that is what makes the theme survive navigating between the
 * landing page and the docs pages, which previously each owned their own copy
 * of this state and fought over it.
 *
 * Also keeps in step with the two things that can change the theme from
 * outside this component: another tab, and the OS switching appearance while
 * the visitor has never made an explicit choice here.
 */
export function useTheme(): [Theme, () => void] {
  const [theme, setTheme] = useState<Theme>(resolveTheme);

  useEffect(() => {
    applyTheme(theme);
  }, [theme]);

  useEffect(() => {
    const onStorage = (event: StorageEvent) => {
      if (event.key !== THEME_STORAGE_KEY) return;
      if (event.newValue === "light" || event.newValue === "dark") {
        setTheme(event.newValue);
      }
    };
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onSystemChange = (event: MediaQueryListEvent) => {
      // Only follow the OS while the visitor has not chosen for themselves;
      // an explicit choice outranks it.
      if (window.localStorage?.getItem(THEME_STORAGE_KEY)) return;
      setTheme(event.matches ? "dark" : "light");
    };

    window.addEventListener("storage", onStorage);
    media.addEventListener("change", onSystemChange);
    return () => {
      window.removeEventListener("storage", onStorage);
      media.removeEventListener("change", onSystemChange);
    };
  }, []);

  const toggle = useCallback(() => {
    setTheme((current) => (current === "dark" ? "light" : "dark"));
  }, []);

  return [theme, toggle];
}
