/**
 * NexTalk Tailwind preset — maps design tokens to Tailwind utility keys.
 * Consumed by both apps' tailwind.config.js via
 * `preset: [require('@nextalk/design-tokens/src/tailwind-preset.js')]`.
 * Values mirror packages/design-tokens/src/tokens.css (UI-SPEC V1.0).
 */
module.exports = {
  theme: {
    extend: {
      colors: {
        portalGreen: '#97ce4c',
        mortyYellow: '#fbf061',
        rickBlue: '#00b5cc',
        darkerSpace: '#151519',
        spaceDark: '#1E1E24',
        panel: '#1A1A22',
      },
      boxShadow: {
        'cartoon-green': '6px 6px 0 0 #97ce4c',
        'cartoon-yellow': '6px 6px 0 0 #fbf061',
        'cartoon-blue': '6px 6px 0 0 #00b5cc',
        'cartoon-black': '4px 4px 0 0 #000',
        'cartoon-black-lg': '8px 8px 0 0 #000',
      },
      fontFamily: {
        sans: ['"Space Grotesk"', '"PingFang SC"', 'system-ui', '-apple-system', 'sans-serif'],
      },
      backgroundImage: {
        'dot-matrix': 'radial-gradient(#4a4a5c 1px, transparent 1px)',
      },
      backgroundSize: {
        'dot-matrix': '20px 20px',
      },
    },
  },
};
