/** @type {import('tailwindcss').Config} */
module.exports = {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
  presets: [require('@nextalk/design-tokens/src/tailwind-preset.js')],
  theme: {
    extend: {},
  },
  plugins: [],
};
