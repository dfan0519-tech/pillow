/**
 * Minimal icon set using the upstream wrap pattern from Grok App icons.tsx.
 * Upstream: src/components/icons.tsx at cbbbdf17bb8db1588b6707f3f42f6b00397e27f9.
 * Tabler is not installed in this package; stroke paths follow Tabler geometry
 * (24×24, stroke icons) so the shell stays consistent without a new dependency.
 */

import type { ReactNode } from "react";

export type IconProps = {
  size?: number;
  title?: string;
  className?: string;
  stroke?: number;
};

function IconShell({
  size = 18,
  title,
  className = "",
  stroke = 1.75,
  children,
}: IconProps & { children: ReactNode }) {
  const classes = ["g-icon", className].filter(Boolean).join(" ");
  return (
    <span
      className={classes}
      style={{
        display: "inline-flex",
        width: size,
        height: size,
        lineHeight: 0,
        color: "currentColor",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
      }}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      title={title}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth={stroke}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        {children}
      </svg>
    </span>
  );
}

/**
 * Compact product mark — 2D möbius ribbon (provisional).
 * Prefer `BrandMark` for full chrome; this keeps the IconProps call-site shape.
 * Final 3D mark should replace the SVG when design exports land.
 */
export function IconPillowMark({ size = 20, title, className = "" }: IconProps) {
  const classes = ["g-icon", "g-icon--pillow-mark", className].filter(Boolean).join(" ");
  const gradId = "pillow-icon-mark-grad";
  return (
    <span
      className={classes}
      style={{
        display: "inline-flex",
        width: size,
        height: size,
        lineHeight: 0,
        color: "currentColor",
        flexShrink: 0,
        alignItems: "center",
        justifyContent: "center",
      }}
      role={title ? "img" : undefined}
      aria-hidden={title ? undefined : true}
      aria-label={title}
      title={title}
    >
      <svg
        width={size}
        height={size}
        viewBox="0 0 24 24"
        fill="none"
        aria-hidden
      >
        <defs>
          <linearGradient id={gradId} x1="0%" y1="30%" x2="100%" y2="70%">
            <stop offset="0%" stopColor="#DCCBFF" />
            <stop offset="50%" stopColor="#FAF9F7" />
            <stop offset="100%" stopColor="#DCCBFF" />
          </linearGradient>
        </defs>
        <path
          d="M7.6 8.2c2.4 0 3.75 1.5 4.8 2.85.3.38.57.73.85 1.05.28-.32.55-.67.85-1.05 1.05-1.35 2.4-2.85 4.8-2.85 2.7 0 4.7 1.95 4.7 4.4s-2 4.4-4.7 4.4c-2.4 0-3.75-1.5-4.8-2.85-.3-.38-.57-.73-.85-1.05-.28.32-.55.67-.85 1.05-1.05 1.35-2.4 2.85-4.8 2.85-2.7 0-4.7-1.95-4.7-4.4s2-4.4 4.7-4.4zm0 1.7c-1.65 0-2.95 1.2-2.95 2.7s1.3 2.7 2.95 2.7c1.65 0 2.8-1.1 3.75-2.35.25-.32.48-.64.7-.95.22.31.45.63.7.95.95 1.25 2.1 2.35 3.75 2.35 1.65 0 2.95-1.2 2.95-2.7s-1.3-2.7-2.95-2.7c-1.65 0-2.8 1.1-3.75 2.35-.25.32-.48.64-.7.95-.22-.31-.45-.63-.7-.95-.95-1.25-2.1-2.35-3.75-2.35z"
          fill={`url(#${gradId})`}
        />
      </svg>
    </span>
  );
}

export function IconSearch(props: IconProps) {
  return (
    <IconShell {...props}>
      <circle cx="11" cy="11" r="7" />
      <path d="m20 20-3.5-3.5" />
    </IconShell>
  );
}

export function IconNewChat(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="M8 9h8" />
      <path d="M8 13h5" />
      <path d="M12 21l-3-3H7a4 4 0 0 1-4-4V8a4 4 0 0 1 4-4h10a4 4 0 0 1 4 4v6a4 4 0 0 1-4 4h-1z" />
      <path d="M19 3v4" />
      <path d="M17 5h4" />
    </IconShell>
  );
}

export function IconFolder(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="M5 6h4l2 2h8a2 2 0 0 1 2 2v7a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2z" />
    </IconShell>
  );
}

export function IconFolderPlus(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="M12 19H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h4l2 2h6a2 2 0 0 1 2 2v2" />
      <path d="M16 19h6" />
      <path d="M19 16v6" />
    </IconShell>
  );
}

export function IconChevronDown(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="m6 9 6 6 6-6" />
    </IconShell>
  );
}

export function IconChevronRight(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="m9 6 6 6-6 6" />
    </IconShell>
  );
}

export function IconPanelRight(props: IconProps) {
  return (
    <IconShell {...props}>
      <rect x="4" y="5" width="16" height="14" rx="2" />
      <path d="M15 5v14" />
    </IconShell>
  );
}

export function IconSend(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="M10 14 21 3" />
      <path d="M21 3l-6.5 18a.55.55 0 0 1-1 0L10 14l-7-3.5a.55.55 0 0 1 0-1L21 3" />
    </IconShell>
  );
}

export function IconClose(props: IconProps) {
  return (
    <IconShell {...props}>
      <path d="M18 6 6 18" />
      <path d="m6 6 12 12" />
    </IconShell>
  );
}

export function IconCircleDot(props: IconProps) {
  return (
    <IconShell {...props}>
      <circle cx="12" cy="12" r="8" />
      <circle cx="12" cy="12" r="3" fill="currentColor" stroke="none" />
    </IconShell>
  );
}

export function IconTerminal(props: IconProps) {
  return (
    <IconShell {...props}>
      <rect x="3" y="5" width="18" height="14" rx="2" />
      <path d="m7 9 3 3-3 3" />
      <path d="M13 15h4" />
    </IconShell>
  );
}

export function IconLock(props: IconProps) {
  return (
    <IconShell {...props}>
      <rect x="5" y="10" width="14" height="10" rx="2" />
      <path d="M8 10V7a4 4 0 0 1 8 0v3" />
    </IconShell>
  );
}
