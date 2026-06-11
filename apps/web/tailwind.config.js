/** Supabase-style dark theme: zinc surface scale + emerald brand accent */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        surface: { DEFAULT: "#09090b", 1: "#101012", 2: "#18181b", 3: "#1f1f23" },
        line: "#27272a",
        brand: { DEFAULT: "#3ecf8e", dim: "#10b981", deep: "#065f46" },
        up: "#3ecf8e", down: "#f87171",
      },
      fontFamily: {
        sans: ["Inter", "system-ui", "sans-serif"],
        mono: ["JetBrains Mono", "ui-monospace", "monospace"],
      },
    },
  },
  plugins: [],
};
