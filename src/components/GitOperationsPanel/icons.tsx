import type { Component } from "solid-js";

/** Monochrome inline SVG icons for the Git Operations Panel.
 *  All icons use fill="currentColor" to inherit text color per STYLE_GUIDE. */

export const PullIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 12l-4-4h2.5V3h3v5H12L8 12z" />
  </svg>
);

export const PushIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 3l4 4H9.5v5h-3V7H4l4-4z" />
  </svg>
);

export const FetchIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 2a6 6 0 100 12A6 6 0 008 2zm0 1a5 5 0 014.9 4H11l-3 3.5L5 7H3.1A5 5 0 018 3z" />
  </svg>
);

export const BranchIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M11.75 5a1.25 1.25 0 10-2.5 0 1.25 1.25 0 001.25 1.25v3.5A1.25 1.25 0 1011.75 11V8c0-.55-.45-1-1-1H8.5a2 2 0 01-2-2V4.75a1.25 1.25 0 10-1.25 0V7a2 2 0 002 2h2.25v2a1.25 1.25 0 101.25 0V6.25A1.25 1.25 0 0011.75 5zM5.25 3.75a.75.75 0 110 1.5.75.75 0 010-1.5zm5.25.5a.75.75 0 110 1.5.75.75 0 010-1.5zm0 6.5a.75.75 0 110 1.5.75.75 0 010-1.5z" />
  </svg>
);

export const MergeIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M5 3a1.5 1.5 0 00-.75 2.8v4.4a1.5 1.5 0 101.5 0V8.5c.75.75 1.75 1.25 3 1.5v.2a1.5 1.5 0 101.5 0v-.45c0-.3-.2-.55-.5-.6C7.5 8.85 6.25 7.5 5.75 5.8A1.5 1.5 0 005 3zm0 .75a.75.75 0 110 1.5.75.75 0 010-1.5zm0 7a.75.75 0 110 1.5.75.75 0 010-1.5zm5.25.5a.75.75 0 110 1.5.75.75 0 010-1.5z" />
  </svg>
);

export const StashIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M3 3h10v2H3V3zm1 3h8v2H4V6zm1 3h6v2H5V9zm1 3h4v2H6v-2z" />
  </svg>
);

export const NewBranchIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M11 2.5V5h2.5v1H11v2.5h-1V6H7.5V5H10V2.5h1zM5.25 3.75a1.25 1.25 0 10-.001 2.501A1.25 1.25 0 005.25 3.75zm0 .5a.75.75 0 110 1.5.75.75 0 010-1.5zm0 6a1.25 1.25 0 100 2.5 1.25 1.25 0 000-2.5zm0 .5a.75.75 0 110 1.5.75.75 0 010-1.5zm.5-4.75v4.25h-1V6h1z" />
  </svg>
);

export const WarningIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M7.56 1.44l-6.5 11.5A.5.5 0 001.5 14h13a.5.5 0 00.44-.76l-6.5-11.5a.5.5 0 00-.88 0zM8 5a.5.5 0 01.5.5v3a.5.5 0 01-1 0v-3A.5.5 0 018 5zm0 5.5a.75.75 0 110 1.5.75.75 0 010-1.5z" />
  </svg>
);

export const CheckIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M13.78 4.22a.75.75 0 010 1.06l-7.25 7.25a.75.75 0 01-1.06 0L2.22 9.28a.75.75 0 011.06-1.06L6 10.94l6.72-6.72a.75.75 0 011.06 0z" />
  </svg>
);

export const ErrorIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 1a7 7 0 100 14A7 7 0 008 1zm0 1a6 6 0 110 12A6 6 0 018 2zm2.85 3.15a.5.5 0 01.7.7L9.21 8.19l2.34 2.34a.5.5 0 01-.7.7L8.5 8.9l-2.35 2.34a.5.5 0 11-.7-.7L7.79 8.19 5.44 5.85a.5.5 0 01.7-.7L8.5 7.49l2.35-2.34z" />
  </svg>
);

export const CloseIcon: Component = () => (
  <svg viewBox="0 0 16 16" fill="currentColor">
    <path d="M8 8.707l3.646 3.647.708-.707L8.707 8l3.647-3.646-.707-.708L8 7.293 4.354 3.646l-.708.708L7.293 8l-3.647 3.646.708.708L8 8.707z" />
  </svg>
);
