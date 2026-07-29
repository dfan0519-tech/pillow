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

/** Quiet product mark — geometric K monogram (functional brand glyph). */
export function IconKeeperMark({ size = 20, title = "Keeper", className = "" }: IconProps) {
  const classes = ["g-icon", "g-icon--keeper-mark", className].filter(Boolean).join(" ");
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
        <rect
          x="3"
          y="3"
          width="18"
          height="18"
          rx="5"
          fill="currentColor"
          opacity="0.12"
        />
        <path
          d="M8.25 7h2.1v4.05L14.9 7h2.55l-4.15 4.35L17.7 17h-2.6l-3.75-4.7V17h-2.1V7z"
          fill="currentColor"
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
