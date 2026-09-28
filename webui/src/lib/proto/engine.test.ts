import { describe, expect, it } from 'vitest';
import { InsertMode, ImageMode } from './engine';

describe('generated proto types', () => {
    it('enums match engine contract', () => {
        expect(InsertMode.INTO).toBe(0);
        expect(InsertMode.BEFORE).toBe(1);
        expect(InsertMode.AFTER).toBe(2);
        expect(ImageMode.READ).toBe(0);
        expect(ImageMode.WRITE).toBe(1);
    });
});
