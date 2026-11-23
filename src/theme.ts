import type { Appearance } from "./types";

export function applyTheme(appearance: Appearance): void {
  const theme =
    appearance === "system"
      ? window.matchMedia("(prefers-color-scheme: light)").matches
        ? "light"
        : "dark"
      : appearance;
  document.documentElement.dataset.theme = theme;
}
