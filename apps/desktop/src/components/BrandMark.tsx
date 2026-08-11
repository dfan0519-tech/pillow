/**
 * Pillow brand mark — Concept 08 «Mirage Loop».
 *
 * The design source is a 3D-rendered möbius-style ribbon. This component is a
 * layered 2D SVG approximation (multi-path faces, cross depth, specular rim,
 * soft bloom) for product chrome only.
 *
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

/** Detail ladder: compact for sidebar, standard for about/chrome, hero for icons. */
type MarkDetail = "compact" | "standard" | "hero";

function markDetail(size: number): MarkDetail {
  // `compact` flattens the ribbon into two plain lobes. That is a different
  // drawing, not a lighter one, and at the 22px the sidebar uses it read as a
  // thinner logo than the taskbar icon. Reserve it for genuinely tiny sizes so
  // every surface a person actually looks at shows the same segmented mark.
  if (size < 18) return "compact";
  if (size < 80) return "standard";
  return "hero";
}

/**
 * Asymmetric ∞ centerline segments (viewBox 0 0 100 55).
 *
 * Topology (continuous ribbon):
 *   left apex → left top → overpass (NW→SE) → right bottom → right apex
 *   → right top → underpass (NE→SW) → left bottom → left apex
 *
 * Left lobe: smaller + rounder. Right lobe: larger + flatter.
 * Band width ~14–18% of mark height via stroke widths (face-on wider, side narrower).
 */
const PATH = {
  /** Left apex → NW of cross (light face, face-on width). */
  leftTop:
    "M17 27.5 C17.5 18 24 11.5 33 12.2 C39 12.8 42.5 16.2 45 19.5",
  /** Overpass NW→SE (on top of underpass). */
  overpass: "M45 19.5 C48 23.8 52 31.2 55 35.5",
  /** SE → right apex (silver / smoke face). */
  rightBottom:
    "M55 35.5 C59.5 39.5 68 42.5 78 40.2 C84.5 38.5 88.5 33.2 90 27.5",
  /** Right apex → NE (dark face after outer twist). */
  rightTop:
    "M90 27.5 C88.5 21.8 84 16.2 76 14.2 C68 12.2 60.5 14.2 55 19.5",
  /** Underpass NE→SW (drawn under overpass). */
  underpass: "M55 19.5 C52 23.8 48 31.2 45 35.5",
  /** SW → left apex (dark → lilac transition). */
  leftBottom:
    "M45 35.5 C41.5 39.5 34 43.5 26 41.5 C20 39.8 17.5 33.5 17 27.5",
  /** Compact: full left lobe (NW → left apex → SW). */
  leftLoop:
    "M45 19.5 C42.5 16.2 39 12.8 33 12.2 C24 11.5 17.5 18 17 27.5 C17.5 33.5 20 39.8 26 41.5 C34 43.5 41.5 39.5 45 35.5",
  /** Compact: full right lobe (SE → right apex → NE). */
  rightLoop:
    "M55 35.5 C59.5 39.5 68 42.5 78 40.2 C84.5 38.5 88.5 33.2 90 27.5 C88.5 21.8 84 16.2 76 14.2 C68 12.2 60.5 14.2 55 19.5",
  /** Compact hue wash along light path (lilac → white → smoke). */
  hueWash:
    "M17 27.5 C17.5 18 24 11.5 33 12.2 C39 12.8 42.5 16.2 45 19.5 C48 23.8 52 31.2 55 35.5 C59.5 39.5 68 42.5 78 40.2 C84.5 38.5 88.5 33.2 90 27.5",
  /** Soft bloom silhouette (filled, blurred). Asymmetric double lobe. */
  glowSilhouette:
    "M18 27.5 C18 15.5 28.5 9.5 40 14.5 C45 17.2 47.5 22.5 50 27.5 C52.5 22.5 55 17.2 60 14.5 C73 8.5 90.5 14 90.5 27.5 C90.5 41 73 46.5 60 40.5 C55 37.8 52.5 32.5 50 27.5 C47.5 32.5 45 37.8 40 40.5 C28.5 45.5 18 39.5 18 27.5 Z",
  /** Outer-edge specular hints (arc tops where faces turn). */
  rimLeft: "M20 18.5 C24 13.5 30 11.2 36 12.5 C40 13.5 43 16 44.5 18.5",
  rimRight: "M55.5 18.5 C58 15.5 65 12.5 74 13.5 C82 14.5 87.5 19 89 24",
} as const;

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
  const detail = markDetail(size);
  const classes = ["brand-symbol", `brand-symbol--${detail}`, className]
    .filter(Boolean)
    .join(" ");

  const g = {
    light: `${uid}-light`,
    dark: `${uid}-dark`,
    lilac: `${uid}-lilac`,
    smoke: `${uid}-smoke`,
    pathHue: `${uid}-path`,
    glow: `${uid}-glow`,
    bloom: `${uid}-bloom`,
    crossShadow: `${uid}-xshadow`,
  };

  // Stroke widths: face-on ≈ 16% of viewBox height (55 → ~8.8), side ~14% → ~5.5
  // Weights match `src-tauri/icons/pillow-mark.svg`. The icon and this component
  // must read as one mark; when they drifted the app chrome looked like a
  // thinner, different logo than the taskbar.
  const w = {
    face: 10.0,
    side: 6.8,
    cross: 7.2,
    rim: detail === "compact" ? 0 : detail === "hero" ? 0.55 : 0.45,
    shadow: detail === "compact" ? 8.2 : 7.6,
  };

  const strokeCommon = {
    fill: "none" as const,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
  };

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
        viewBox="0 0 100 55"
        fill="none"
        aria-hidden
        className="brand-symbol__svg"
      >
        <defs>
          {/* Path hue: lilac → near-white → smoke → lilac */}
          <linearGradient id={g.pathHue} x1="8%" y1="35%" x2="92%" y2="65%">
            <stop offset="0%" stopColor="#DCCBFF" />
            <stop offset="28%" stopColor="#FAF9F7" />
            <stop offset="55%" stopColor="#C7C7CE" />
            <stop offset="82%" stopColor="#E4D8F8" />
            <stop offset="100%" stopColor="#DCCBFF" />
          </linearGradient>
          {/* Light ribbon face (near-white / silver). */}
          <linearGradient id={g.light} x1="0%" y1="0%" x2="100%" y2="100%">
            <stop offset="0%" stopColor="#F5F2FF" />
            <stop offset="45%" stopColor="#FAF9F7" />
            <stop offset="100%" stopColor="#E8E4F0" />
          </linearGradient>
          {/* Dark ribbon face (lavender → grey-purple). */}
          <linearGradient id={g.dark} x1="100%" y1="0%" x2="0%" y2="100%">
            <stop offset="0%" stopColor="#B8A8D0" />
            <stop offset="50%" stopColor="#8E869C" />
            <stop offset="100%" stopColor="#6E6878" />
          </linearGradient>
          <linearGradient id={g.lilac} x1="0%" y1="50%" x2="100%" y2="50%">
            <stop offset="0%" stopColor="#DCCBFF" />
            <stop offset="100%" stopColor="#EDE4FF" />
          </linearGradient>
          <linearGradient id={g.smoke} x1="0%" y1="40%" x2="100%" y2="70%">
            <stop offset="0%" stopColor="#D4D0DC" />
            <stop offset="55%" stopColor="#C7C7CE" />
            <stop offset="100%" stopColor="#A8A4B0" />
          </linearGradient>
          {/* Soft purple bloom (large blur, low opacity). */}
          <filter
            id={g.bloom}
            x="-50%"
            y="-80%"
            width="200%"
            height="260%"
            colorInterpolationFilters="sRGB"
          >
            <feGaussianBlur
              in="SourceGraphic"
              stdDeviation={detail === "hero" ? 3.2 : detail === "standard" ? 2.4 : 1.6}
              result="blur"
            />
            <feColorMatrix
              in="blur"
              type="matrix"
              values="0.85 0 0 0 0.08
                      0.75 0 0 0 0.05
                      1    0 0 0 0.12
                      0    0 0 0.55 0"
              result="tint"
            />
            <feMerge>
              <feMergeNode in="tint" />
            </feMerge>
          </filter>
          {/* Mild glow merge for body accents. */}
          <filter
            id={g.glow}
            x="-30%"
            y="-50%"
            width="160%"
            height="200%"
            colorInterpolationFilters="sRGB"
          >
            <feGaussianBlur in="SourceGraphic" stdDeviation="0.9" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
          {/* Contact shadow under the overpass (purple-deep grey, not pure black). */}
          <filter
            id={g.crossShadow}
            x="-40%"
            y="-60%"
            width="180%"
            height="220%"
            colorInterpolationFilters="sRGB"
          >
            <feGaussianBlur in="SourceGraphic" stdDeviation="1.1" result="blur" />
            <feMerge>
              <feMergeNode in="blur" />
              <feMergeNode in="SourceGraphic" />
            </feMerge>
          </filter>
        </defs>

        {/* ── 0. Outline (lowest) ───────────────────────────────────────
            Same pass as the app icon. On dark chrome it merges into the
            background; on light chrome it holds the silhouette. Both surfaces
            then adapt the same way, which is what makes them one mark rather
            than two similar ones. */}
        <g
          fill="none"
          stroke="#2A2733"
          strokeLinecap="round"
          strokeLinejoin="round"
          opacity={0.92}
          className="brand-symbol__outline"
        >
          <path d={PATH.underpass} strokeWidth={w.cross + 2.4} />
          <path d={PATH.leftBottom} strokeWidth={w.side + 2.4} />
          <path d={PATH.rightTop} strokeWidth={w.side + 2.4} />
          <path d={PATH.rightBottom} strokeWidth={w.face + 2.4} />
          <path d={PATH.leftTop} strokeWidth={w.face + 2.4} />
          <path d={PATH.overpass} strokeWidth={w.cross + 2.4} />
        </g>

        {/* ── 1. Bloom ──────────────────────────────────────────────────── */}
        <path
          d={PATH.glowSilhouette}
          className="brand-symbol__bloom"
          fill="#DCCBFF"
          opacity={0.22}
          filter={`url(#${g.bloom})`}
        />

        {/* ── 2. Underpass + contact shadow (back strand) ───────────────── */}
        <path
          d={PATH.underpass}
          {...strokeCommon}
          stroke="#4A4458"
          strokeWidth={w.shadow}
          opacity={detail === "compact" ? 0.5 : 0.62}
          filter={`url(#${g.crossShadow})`}
          className="brand-symbol__cross-shadow"
        />
        <path
          d={PATH.underpass}
          {...strokeCommon}
          stroke={`url(#${g.dark})`}
          strokeWidth={w.cross}
          className="brand-symbol__underpass"
        />

        {detail === "compact" ? (
          <>
            {/* Compact: two lobe bodies + path hue; still over/under at cross */}
            <path
              d={PATH.leftLoop}
              {...strokeCommon}
              stroke={`url(#${g.lilac})`}
              strokeWidth={w.face}
              className="brand-symbol__left-loop"
            />
            <path
              d={PATH.rightLoop}
              {...strokeCommon}
              stroke={`url(#${g.smoke})`}
              strokeWidth={w.face * 0.95}
              className="brand-symbol__right-loop"
            />
          </>
        ) : (
          <>
            {/* ── 3. Dark faces (möbius underside) ─────────────────────── */}
            <path
              d={PATH.leftBottom}
              {...strokeCommon}
              stroke={`url(#${g.dark})`}
              strokeWidth={w.face * 0.92}
              className="brand-symbol__face-dark brand-symbol__left-bottom"
            />
            <path
              d={PATH.rightTop}
              {...strokeCommon}
              stroke={`url(#${g.dark})`}
              strokeWidth={w.side}
              className="brand-symbol__face-dark brand-symbol__right-top"
            />

            {/* ── 4. Light / silver faces ──────────────────────────────── */}
            <path
              d={PATH.leftTop}
              {...strokeCommon}
              stroke={`url(#${g.light})`}
              strokeWidth={w.face}
              filter={detail === "hero" ? `url(#${g.glow})` : undefined}
              className="brand-symbol__face-light brand-symbol__left-top"
            />
            <path
              d={PATH.rightBottom}
              {...strokeCommon}
              stroke={`url(#${g.smoke})`}
              strokeWidth={w.face * 0.96}
              className="brand-symbol__face-light brand-symbol__right-bottom"
            />
          </>
        )}

        {/* ── 5. Overpass (front strand — establishes depth) ──────────── */}
        <path
          d={PATH.overpass}
          {...strokeCommon}
          stroke={`url(#${g.light})`}
          strokeWidth={w.cross}
          className="brand-symbol__overpass"
        />
        {/* Thin bright edge on overpass for specular catch */}
        <path
          d={PATH.overpass}
          {...strokeCommon}
          stroke="#FAF9F7"
          strokeWidth={w.cross * 0.38}
          opacity={0.55}
          className="brand-symbol__overpass-spec"
        />

        {/* ── 6. Specular rim on outer arc tops (face turn) ────────────── */}
        {w.rim > 0 ? (
          <g className="brand-symbol__rim" opacity={detail === "hero" ? 0.75 : 0.55}>
            <path
              d={PATH.rimLeft}
              {...strokeCommon}
              stroke="#FFFFFF"
              strokeWidth={w.rim}
            />
            <path
              d={PATH.rimRight}
              {...strokeCommon}
              stroke="#FFFFFF"
              strokeWidth={w.rim}
            />
          </g>
        ) : null}

        {/* Compact: subtle path-hue wash so lilac→white→smoke reads at 22px */}
        {detail === "compact" ? (
          <path
            d={PATH.hueWash}
            {...strokeCommon}
            stroke={`url(#${g.pathHue})`}
            strokeWidth={w.face * 0.35}
            opacity={0.45}
            className="brand-symbol__hue-wash"
          />
        ) : null}
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
