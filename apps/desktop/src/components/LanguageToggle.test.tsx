import { fireEvent, render, screen, within } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import LanguageToggle from './LanguageToggle';

describe('LanguageToggle', () => {
  it('exposes the three segments as pressed buttons inside a labelled group', () => {
    render(<LanguageToggle speaker="interviewer" value="bilingual" onChange={() => undefined} />);

    const group = screen.getByRole('group', { name: '面试官语言' });
    const segments = within(group).getAllByRole('button');

    expect(segments.map((segment) => segment.textContent)).toEqual(['中', 'EN', 'EN+中']);
    expect(segments.map((segment) => segment.getAttribute('aria-pressed'))).toEqual([
      'false',
      'false',
      'true',
    ]);
  });

  it('fills the selected segment with the speaker brand color', () => {
    const { rerender } = render(
      <LanguageToggle speaker="user" value="all-zh" onChange={() => undefined} />,
    );
    expect(screen.getByRole('button', { name: '中', pressed: true }).className).toContain(
      'bg-portalGreen',
    );

    rerender(<LanguageToggle speaker="interviewer" value="bilingual" onChange={() => undefined} />);
    expect(screen.getByRole('button', { name: 'EN+中', pressed: true }).className).toContain(
      'bg-gray-600',
    );
  });

  it('reports the LanguagePref of the segment the user picks', () => {
    const onChange = vi.fn();
    render(<LanguageToggle speaker="user" value="all-zh" onChange={onChange} />);

    fireEvent.click(screen.getByRole('button', { name: 'EN' }));
    expect(onChange).toHaveBeenLastCalledWith('all-en');

    fireEvent.click(screen.getByRole('button', { name: 'EN+中' }));
    expect(onChange).toHaveBeenLastCalledWith('bilingual');

    fireEvent.click(screen.getByRole('button', { name: '中' }));
    expect(onChange).toHaveBeenLastCalledWith('all-zh');
  });
});
