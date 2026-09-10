/**
 * Mock data for the Phase 1 desktop UI slice.
 *
 * Every value here stands in for a real asset that ships in a later phase
 * (resume indexing is Phase 5, dual-track recording and the review report are
 * Phase 6, the voice clone is Phase 2). Anything rendered from this module is
 * labelled with MOCK_BADGE_LABEL (模拟数据) so a demo viewer can never mistake
 * it for real user data (UI-SPEC Missing Pages Contract).
 */

/** Badge text marking demo data — locked copy, UI-SPEC Copywriting Contract. */
export const MOCK_BADGE_LABEL = '模拟数据';

export interface MockResume {
  id: string;
  fileName: string;
  sizeLabel: string;
  /** Experience lines the mock resume "contains" (Phase 5 will parse these). */
  highlights: string[];
}

/** One mock resume so the console knowledge row and 简历导入 page agree. */
export const MOCK_RESUME: MockResume = {
  id: 'resume-mock-01',
  fileName: '模拟简历.pdf',
  sizeLabel: '248 KB',
  highlights: ['5 年后端开发经验', '高并发订单系统重构', 'K8s 集群运维'],
};

export interface MockGlossaryTerm {
  id: string;
  term: string;
  /** Grouping tag rendered as a colored chip on the glossary row. */
  category: GlossaryCategory;
}

/** Categories are mock groupings; Phase 4 wires real STT/MT term protection. */
export type GlossaryCategory = '工具' | '架构' | '系统';

export const GLOSSARY_CATEGORIES: readonly GlossaryCategory[] = ['工具', '架构', '系统'];

/** Three terms the copy contract itself uses as examples (K8s, 幂等性). */
export const MOCK_GLOSSARY_TERMS: readonly MockGlossaryTerm[] = [
  { id: 'term-mock-01', term: 'K8s', category: '工具' },
  { id: 'term-mock-02', term: '幂等性', category: '架构' },
  { id: 'term-mock-03', term: 'backpressure', category: '系统' },
];
