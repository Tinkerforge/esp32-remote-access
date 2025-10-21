import { describe, it, expect } from 'vitest';
import { evaluatePasswordStrength, PasswordStrength } from '../passwordStrength';

describe('Password Strength Utilities', () => {
    describe('evaluatePasswordStrength', () => {
        it('should rate very weak passwords', () => {
            const result = evaluatePasswordStrength('12345');
            expect(result.strength).toBe(PasswordStrength.VeryWeak);
            expect(result.color).toBe('#dc3545'); // danger red
        });

        it('should rate weak passwords', () => {
            const result = evaluatePasswordStrength('abc12');
            expect(result.strength).toBe(PasswordStrength.Weak);
            expect(result.color).toBe('#fd7e14'); // warning orange
        });

        it('should rate fair passwords', () => {
            const result = evaluatePasswordStrength('passs2or');
            expect(result.strength).toBe(PasswordStrength.Fair);
            expect(result.color).toBe('#ffc107'); // warning yellow
        });

        it('should rate strong passwords', () => {
            const result = evaluatePasswordStrength('MyP@ssw0rd!2024');
            expect(result.strength).toBe(PasswordStrength.Strong);
            expect(result.color).toBe('#28a745'); // success green
        });

        it('should rate very strong passwords', () => {
            const result = evaluatePasswordStrength('MyVery$ecureP@ssw0rd!WithM@nyChar$2024');
            expect(result.strength).toBe(PasswordStrength.VeryStrong);
            expect(result.color).toBe('#20c997'); // teal/cyan
        });

        it('should calculate percentage correctly', () => {
            const veryWeak = evaluatePasswordStrength('abc');
            expect(veryWeak.percentage).toBeLessThanOrEqual(20);

            const weak = evaluatePasswordStrength('abc12');
            expect(weak.percentage).toBeGreaterThan(20);
            expect(weak.percentage).toBeLessThanOrEqual(40);

            const fair = evaluatePasswordStrength('passs2or');
            expect(fair.percentage).toBeGreaterThan(40);
            expect(fair.percentage).toBeLessThanOrEqual(60);

            const strong = evaluatePasswordStrength('MyP@ssw0rd!2024');
            expect(strong.percentage).toBeGreaterThan(60);
            expect(strong.percentage).toBeLessThanOrEqual(80);
        });
    });
});
