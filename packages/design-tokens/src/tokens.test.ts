import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// Read the contract file directly: vitest stubs .css imports (incl. ?raw) by default.
const cssText = readFileSync(fileURLToPath(new URL('./tokens.css', import.meta.url)), 'utf8');

/** Parse custom properties declared inside `:root { ... }` into a name → value map. */
function parseTokens(css: string): Map<string, string> {
  const map = new Map<string, string>();
  const block = css.match(/:root\s*\{([^}]*)\}/)?.[1];
  if (!block) return map;
  // Strip comments first — a declaration right after `*/` would otherwise
  // be glued to the comment text by the split below.
  const clean = block.replace(/\/\*[\s\S]*?\*\//g, '');
  for (const decl of clean.split(';')) {
    const m = decl.match(/^\s*(--[\w-]+)\s*:\s*(.+?)\s*$/);
    if (m) map.set(m[1], m[2].trim());
  }
  return map;
}

describe('color tokens (portalGreen contract snapshot vs UI-SPEC)', () => {
  const t = parseTokens(cssText);

  it('declares the dominant dark-space neutrals', () => {
    expect(t.get('--color-darker-space')).toBe('#151519');
    expect(t.get('--color-space-dark')).toBe('#1E1E24');
    expect(t.get('--color-panel')).toBe('#1A1A22');
  });

  it('declares the slate secondary surfaces', () => {
    expect(t.get('--color-slate-800')).toBe('#1E293B');
    expect(t.get('--color-slate-700')).toBe('#334155');
  });

  it('declares the functional trio portalGreen / mortyYellow / rickBlue', () => {
    expect(t.get('--color-portal-green')).toBe('#97ce4c');
    expect(t.get('--color-morty-yellow')).toBe('#fbf061');
    expect(t.get('--color-rick-blue')).toBe('#00b5cc');
  });

  it('declares the destructive red, ink and paper', () => {
    expect(t.get('--color-red')).toBe('#ef4444');
    expect(t.get('--color-ink')).toBe('#000000');
    expect(t.get('--color-paper')).toBe('#FFFFFF');
  });
});

describe('hard shadow tokens (spec §5.1 offsets)', () => {
  const t = parseTokens(cssText);

  it('declares the black 4px and 8px hard shadows', () => {
    expect(t.get('--shadow-black-4')).toBe('4px 4px 0 0 #000');
    expect(t.get('--shadow-black-8')).toBe('8px 8px 0 0 #000');
  });

  it('declares the 6px colored hard shadows per functional color', () => {
    expect(t.get('--shadow-green-6')).toBe('6px 6px 0 0 #97ce4c');
    expect(t.get('--shadow-yellow-6')).toBe('6px 6px 0 0 #fbf061');
    expect(t.get('--shadow-blue-6')).toBe('6px 6px 0 0 #00b5cc');
  });
});

describe('type scale (spec-locked 5 sizes / 3 weights)', () => {
  const t = parseTokens(cssText);

  it('declares the panel title 18px/28px/700', () => {
    expect(t.get('--text-panel-title')).toBe('18px');
    expect(t.get('--leading-panel-title')).toBe('28px');
    expect(t.get('--weight-panel-title')).toBe('700');
  });

  it('declares dialogue 15px/600, translation 13px/700', () => {
    expect(t.get('--text-dialogue')).toBe('15px');
    expect(t.get('--weight-dialogue')).toBe('600');
    expect(t.get('--text-translation')).toBe('13px');
    expect(t.get('--weight-translation')).toBe('700');
  });

  it('declares label 12px/16px/700 and mini 10px/14px/700', () => {
    expect(t.get('--text-label')).toBe('12px');
    expect(t.get('--leading-label')).toBe('16px');
    expect(t.get('--weight-label')).toBe('700');
    expect(t.get('--text-mini')).toBe('10px');
    expect(t.get('--leading-mini')).toBe('14px');
    expect(t.get('--weight-mini')).toBe('700');
  });

  it('leads the font stack with Space Grotesk', () => {
    expect(t.get('--font-sans')?.startsWith("'Space Grotesk'")).toBe(true);
  });
});

describe('spacing scale (4px multiplier, spec §4.2)', () => {
  const t = parseTokens(cssText);

  it('declares xs→3xl multiples of 4', () => {
    expect(t.get('--space-xs')).toBe('4px');
    expect(t.get('--space-sm')).toBe('8px');
    expect(t.get('--space-md')).toBe('16px');
    expect(t.get('--space-lg')).toBe('20px');
    expect(t.get('--space-2lg')).toBe('24px');
    expect(t.get('--space-xl')).toBe('32px');
    expect(t.get('--space-2xl')).toBe('48px');
    expect(t.get('--space-3xl')).toBe('64px');
  });
});

describe('radii and dot-matrix texture', () => {
  const t = parseTokens(cssText);

  it('declares the shape-consistency radii', () => {
    expect(t.get('--radius-card')).toBe('12px');
    expect(t.get('--radius-window')).toBe('24px');
    expect(t.get('--radius-mobile')).toBe('40px');
    expect(t.get('--radius-full')).toBe('9999px');
  });

  it('declares the root dot-matrix background values', () => {
    expect(t.get('--bg-dot-matrix')).toBe('radial-gradient(#4a4a5c 1px, transparent 1px)');
    expect(t.get('--bg-dot-matrix-size')).toBe('20px 20px');
  });
});
