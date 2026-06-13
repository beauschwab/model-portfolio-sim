/** Binance dark theme: near-black canvas, single yellow accent, trading green/red.
 *  Tokens follow getdesign Binance DESIGN.md (see repo root). */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // near-black canvas + flat color-block surfaces (no atmospheric gradients)
        surface: { DEFAULT: "#0b0e11", 1: "#1e2329", 2: "#2b3139", 3: "#363c45" },
        line: "#2b3139",
        ink: "#181a20", // black text on yellow CTAs (on-primary)
        paper: { DEFAULT: "#eaecef", dim: "#929aa5", faint: "#707a8a" },
        // Binance yellow does ALL brand voltage; dim = press/hover, deep = disabled dark-yellow
        brand: { DEFAULT: "#fcd535", dim: "#f0b90b", deep: "#3a3a1f" },
        up: "#0ecb81", down: "#f6465d",
        info: "#3b82f6", turquoise: "#2dbdb6",
      },
      fontFamily: {
        // BinanceNova → Inter (display + body); BinancePlex → JetBrains Mono (numbers)
        display: ["Inter", "system-ui", "sans-serif"],
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
    },
  },
  plugins: [],
};
