/**
 * The GLIMPS wordmark for prose — set like a live terminal token: mono,
 * ending in the ▌ block cursor. Hovering retypes the word.
 *
 * `quiet` is for dense docs text: the cursor sits static and dim (no blink)
 * so thirty instances on a page don't strobe; it wakes on hover.
 *
 * Styles live in styles.css under "Brand wordmark". Reduced-motion users
 * get the static word + cursor, no animation.
 */
export function Glimps({ quiet = false }: { quiet?: boolean }) {
  return (
    <span className={quiet ? "glimps-word glimps-word--quiet" : "glimps-word"}>
      <span className="glimps-word-inner">GLIMPS</span>
    </span>
  );
}
