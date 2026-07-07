/**
 * Single source of truth for the site's public origin. Change this one line if
 * the domain changes (also update the 3 static files that can't import it:
 * index.html, public/sitemap.xml, public/robots.txt).
 */
export const SITE_URL = "https://glimpps.netlify.app";

/** Absolute canonical URL for a route path (e.g. "/about" or "/"). */
export function canonical(path: string): string {
  if (path === "/") return `${SITE_URL}/`;
  return SITE_URL + path.replace(/\/$/, "");
}

/** Absolute URL for a public asset (e.g. "/og.png"). */
export function asset(path: string): string {
  return SITE_URL + path;
}
