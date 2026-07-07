/**
 * The GLIMPS app mark ("Reveal"): the ▌ is the seam where raw bytes (dim,
 * left) resolve into legible, colored structure (right) — the product, drawn.
 * Self-contained dark tile, so it reads on both light and dark grounds.
 * Decorative in the header (the "glimps" wordmark carries the name), so it's
 * aria-hidden by default; pass a `title` to give it an accessible label.
 */
export function GlimpsMark({
  size = 24,
  className,
  title,
}: {
  size?: number;
  className?: string;
  title?: string;
}) {
  return (
    <svg
      viewBox="0 0 64 64"
      width={size}
      height={size}
      className={className}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
    >
      {title ? <title>{title}</title> : null}
      <rect width="64" height="64" rx="15" fill="#1b1c22" />
      <rect x="10" y="21" width="11" height="4" rx="2" fill="#8a8b93" opacity=".5" />
      <rect x="10" y="30" width="15" height="4" rx="2" fill="#8a8b93" opacity=".5" />
      <rect x="10" y="39" width="8" height="4" rx="2" fill="#8a8b93" opacity=".5" />
      <rect x="29" y="13" width="6" height="38" rx="3" fill="#5fd08a" />
      <rect x="40" y="21" width="14" height="4" rx="2" fill="#7fc7ff" />
      <rect x="40" y="30" width="10" height="4" rx="2" fill="#ecebe4" />
      <rect x="40" y="39" width="13" height="4" rx="2" fill="#e5c07b" />
    </svg>
  );
}
