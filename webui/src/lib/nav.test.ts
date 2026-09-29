import { describe, expect, it } from 'vitest';
import { sectionHref } from './nav';

describe('sectionHref', () => {
    it('deep-links sections when image is open', () => {
        expect(sectionHref('image', 'i-1')).toBe('/image/i-1');
        expect(sectionHref('forms', 'i-1')).toBe('/forms/i-1');
        expect(sectionHref('nvar', 'i-1')).toBe('/nvar/i-1');
        expect(sectionHref('snapshots', 'i-1')).toBe('/snapshots/i-1');
    });

    it('index links without image', () => {
        expect(sectionHref('image', null)).toBe('/image');
        expect(sectionHref('forms', undefined)).toBe('/forms');
    });

    it('artifacts always index (session-scoped)', () => {
        expect(sectionHref('artifacts', 'i-1')).toBe('/artifacts');
    });
});
