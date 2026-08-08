/**
 * Pillow brand mark — Concept 08 «Mirage Loop».
 *
 * The design source is a 3D-rendered möbius-style ribbon. This component is a
 * clean 2D vector interpretation (gradient + soft glow) for product chrome.
 * Final 3D mark assets should be exported from design source and replace this
 * SVG; do not treat the inline paths as production-final brand art.
 */

import type { CSSProperties } from "react";

export type BrandMarkProps = {
  /** Overall mark height in CSS pixels (symbol scales with it). */
  size?: number;
  /** Show wordmark text next to the symbol. */
  showWordmark?: boolean;
  /** Show small tagline under the wordmark (about surfaces only). */
  showTagline?: boolean;
  /** Optional accessible name; decorative when omitted. */
  title?: string;
  className?: string;
};

/** Unique-ish id suffix so multiple marks on one page do not clash gradients. */
let markSeq = 0;

/**
 * 2D möbius / infinity ribbon — provisional product chrome only.
 * Replace with design-exported 3D mark when available.
 */
export function BrandSymbol({
  size = 22,
  title,
  className = "",
}: {
  size?: number;
  title?: string;
  className?: string;
}) {
  markSeq += 1;
  const uid = `pillow-mark-${markSeq}`;
  const gradId = `${uid}-grad`;
  const glowId = `${uid}-glow`;
  const classes = ["brand-symbol", className].filter(Boolean).join(" ");

  return (
    <span
      className={classes}
      style={
        {
          display: "inline-flex",
          width: size,
          height: size * 0.55,
          lineHeight: 0,
          flexShrink: 0,
          alignItems: "center",
          justifyContent: "center",
        } as CSSProperties
      }
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      title={title}
    >
      <svg
        width={size}
        height={size * 0.55}
        viewBox="0 0 48 26"
        fill="none"
        aria-hidden
        className="brand-symbol__svg"
      >
        <defs>
          <linearGradient id={gradId} x1="0%" y1="20%" x2="100%" y2="80%">
            <stop offset="0%" stopColor="#DCCBFF" />
            <stop offset="42%" stopColor="#FAF9F7" />
            <stop offset="58%" stopColor="#EDE4FF" />
            <stop offset="100%" stopColor="#DCCBFF" />
          </linearGradient>
          <filter
            id={glowId}
            x="-40%"
            y="-80%"
            width="180%"
            height="260%"
            colorInterpolationFilters="sRGB"
          >
            <feGaussianBlur in="SourceGraphic" stdDeviation="1.1" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>
        {/* Soft ambient glow layer */}
        <path
          d="M8.5 13c0-4.2 3.1-7.2 7.2-7.2 2.6 0 4.6 1.1 6.3 2.9L24 11l2-2.3c1.7-1.8 3.7-2.9 6.3-2.9 4.1 0 7.2 3 7.2 7.2s-3.1 7.2-7.2 7.2c-2.6 0-4.6-1.1-6.3-2.9L24 15l-2 2.3c-1.7 1.8-3.7 2.9-6.3 2.9-4.1 0-7.2-3-7.2-7.2z"
          fill={`url(#${gradId})`}
          opacity="0.28"
          filter={`url(#${glowId})`}
        />
        {/* Band body — continuous infinity ribbon with thickness */}
        <path
          d="M15.7 5.8c4.55 0 7.1 2.85 9.05 5.35.55.7 1.05 1.35 1.55 1.95l.2.25c.5-.6 1-1.25 1.55-1.95C29.85 8.65 32.4 5.8 36.95 5.8c5.05 0 8.85 3.7 8.85 8.2s-3.8 8.2-8.85 8.2c-4.55 0-7.1-2.85-9.05-5.35-.55-.7-1.05-1.35-1.55-1.95l-.2-.25c-.5.6-1 1.25-1.55 1.95-1.95 2.5-4.5 5.35-9.05 5.35-5.05 0-8.85-3.7-8.85-8.2s3.8-8.2 8.85-8.2zm0 3.15c-3.05 0-5.5 2.2-5.5 5.05s2.45 5.05 5.5 5.05c3.05 0 5.15-2.05 6.95-4.35.45-.55.85-1.1 1.25-1.6.4.5.8 1.05 1.25 1.6 1.8 2.3 3.9 4.35 6.95 4.35 3.05 0 5.5-2.2 5.5-5.05s-2.45-5.05-5.5-5.05c-3.05 0-5.15 2.05-6.95 4.35-.45.55-.85 1.1-1.25 1.6-.4-.5-.8-1.05-1.25-1.6-1.8-2.3-3.9-4.35-6.95-4.35z"
          fill={`url(#${gradId})`}
        />
        {/* Twist highlight — implies band torsion without 3D mesh */}
        <path
          d="M22.2 11.4c.55.7 1.05 1.35 1.55 1.95.1.12.2.12.3 0 .5-.6 1-1.25 1.55-1.95"
          stroke="#FAF9F7"
          strokeWidth="1.35"
          strokeLinecap="round"
          opacity="0.55"
        />
      </svg>
    </span>
  );
}

/** Lowercase geometric wordmark; lilac tittle on the i is the brand memory point. */
export function BrandWordmark({ className = "" }: { className?: string }) {
  const classes = ["brand-wordmark", className].filter(Boolean).join(" ");
  return (
    <span className={classes} aria-label="pillow">
      {/* Decorative glyph (dotless i + custom tittle); accessible name via aria-label */}
      <span aria-hidden="true">
        p
        <span className="brand-wordmark__i">
          <span className="brand-wordmark__stem">ı</span>
          <span className="brand-wordmark__tittle" />
        </span>
        llow
      </span>
    </span>
  );
}

/**
 * Compact product chrome: symbol + wordmark for the sidebar brand row.
 * Tagline is opt-in for about surfaces only.
 */
export function BrandMark({
  size = 22,
  showWordmark = true,
  showTagline = false,
  title = "pillow",
  className = "",
}: BrandMarkProps) {
  const classes = ["brand-mark", className].filter(Boolean).join(" ");
  return (
    <span className={classes} data-testid="brand-mark">
      <BrandSymbol size={size} title={showWordmark ? undefined : title} />
      {showWordmark ? (
        <span className="brand-mark__text">
          <BrandWordmark />
          {showTagline ? (
            <span className="brand-mark__tagline">SOFTWARE HARNESS TOOL</span>
          ) : null}
        </span>
      ) : null}
    </span>
  );
}
