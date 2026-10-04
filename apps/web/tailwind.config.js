/** Aperture Risk theme: every value resolves to a CSS custom property defined in
 *  src/styles/aperture.css (ported from the Aperture Risk Design System). Colors
 *  use RGB channels so opacity modifiers (bg-brand/15) keep working. See DESIGN.md. */
const rgb = (name) => `rgb(var(--rgb-${name}) / <alpha-value>)`;

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    // Replaced (not extended): Aperture is near-square — 1px controls/panels, 2px
    // modals/large surfaces; only true circles (status dots, toggles) are round.
    borderRadius: {
      none: "0px",
      sm: "var(--radius-sm)",
      DEFAULT: "var(--radius-sm)",
      md: "var(--radius-md)",
      lg: "var(--radius-lg)",
      xl: "var(--radius-lg)",
      "2xl": "var(--radius-lg)",
      full: "var(--radius-full)",
    },
    // Replaced: the dense terminal scale (13px default body).
    fontSize: {
      "2xs": ["var(--text-2xs)", "14px"],
      xs: ["var(--text-xs)", "16px"],
      sm: ["var(--text-sm)", "18px"],
      base: ["var(--text-base)", "20px"],
      md: ["var(--text-md)", "20px"],
      lg: ["var(--text-lg)", "22px"],
      xl: ["var(--text-xl)", "26px"],
      "2xl": ["var(--text-2xl)", "32px"],
      "3xl": ["var(--text-3xl)", "40px"],
      "4xl": ["var(--text-4xl)", "52px"],
    },
    extend: {
      colors: {
        // canvas → base → card → raised → viz/active surfaces
        surface: {
          DEFAULT: rgb("canvas"), base: rgb("base"),
          1: rgb("surface-1"), 2: rgb("surface-2"), 3: rgb("surface-3"), active: rgb("active"), overlay: rgb("overlay"),
        },
        // hairlines: subtle = internal dividers, strong = inputs/cards, heavy = emphasis
        line: { DEFAULT: rgb("border-subtle"), strong: rgb("border-default"), heavy: rgb("border-strong") },
        ink: rgb("on-accent"), // dark text on yellow
        paper: {
          DEFAULT: rgb("primary"), heading: rgb("heading"),
          dim: rgb("secondary"), faint: rgb("tertiary"), disabled: rgb("disabled"),
        },
        // the single accent: primary actions, focus, live, selection — use sparingly
        brand: { DEFAULT: rgb("accent"), hover: rgb("accent-hover"), press: rgb("accent-press") },
        up: rgb("up"), down: rgb("down"),
        warning: rgb("warning"), danger: rgb("danger"), info: rgb("info"),
        ai: rgb("ai"), sky: rgb("sky"),
      },
      fontFamily: {
        display: ["var(--font-sans)"],
        sans: ["var(--font-sans)"],
        // Aperture uses Inter tabular figures for numerics, not a monospace face
        mono: ["var(--font-mono)"],
      },
      letterSpacing: {
        display: "var(--tracking-display)",
        tight: "var(--tracking-tight)",
        snug: "var(--tracking-snug)",
        wide: "var(--tracking-wide)",
        caps: "var(--tracking-caps)",
      },
      spacing: {
        row: "var(--row-height)",
        control: "var(--control-height)",
        "control-sm": "var(--control-height-sm)",
        "control-lg": "var(--control-height-lg)",
      },
      boxShadow: {
        xs: "var(--shadow-xs)",
        sm: "var(--shadow-sm)",
        md: "var(--shadow-md)",
        lg: "var(--shadow-lg)",
        xl: "var(--shadow-overlay)",
        "2xl": "var(--shadow-overlay)",
        focus: "var(--ring-focus)",
        "inset-top": "var(--inset-top-light)",
        ai: "var(--glow-ai)",
      },
      transitionTimingFunction: { DEFAULT: "var(--ease-out)", out: "var(--ease-out)" },
      transitionDuration: { DEFAULT: "150ms", fast: "90ms", base: "150ms", slow: "240ms" },
    },
  },
  plugins: [],
};
