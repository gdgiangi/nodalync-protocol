/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,jsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        // Nodalync brand colors
        nodalync: {
          50: "#f0f4ff",
          100: "#dbe4ff",
          200: "#bac8ff",
          300: "#91a7ff",
          400: "#748ffc",
          500: "#5c7cfa",
          600: "#4c6ef5",
          700: "#4263eb",
          800: "#3b5bdb",
          900: "#364fc7",
        },
        // Entity type colors
        entity: {
          person: "#e599f7",
          organization: "#74c0fc",
          concept: "#69db7c",
          decision: "#ffd43b",
          task: "#ff8787",
          asset: "#a9e34b",
          goal: "#f783ac",
          pattern: "#66d9e8",
          insight: "#b197fc",
        },
      },
    },
  },
  plugins: [],
};
