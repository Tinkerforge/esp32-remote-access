// The async import allows the page to render faster on slow internet connections
const zxcvbn = (await import('zxcvbn')).default;

export enum PasswordStrength {
    VeryWeak = "very_weak",
    Weak = "weak",
    Fair = "fair",
    Strong = "strong",
    VeryStrong = "very_strong",
}

export interface PasswordStrengthInfo {
    strength: PasswordStrength;
    color: string;
    percentage: number;
}

/**
 * Evaluate password strength based on entropy
 *
 * Entropy levels (approximate):
 * - < 28 bits: Very Weak (can be cracked almost instantly)
 * - 28-35 bits: Weak (can be cracked in hours to days)
 * - 36-59 bits: Fair (can be cracked in weeks to months)
 * - 60-127 bits: Strong (can be cracked in years to decades)
 * - >= 128 bits: Very Strong (practically uncrackable with current technology)
 *
 * @param password The password to evaluate
 * @returns Password strength information
 */
export function evaluatePasswordStrength(password: string): PasswordStrengthInfo {
    const result = zxcvbn(password);
    const score = result.score; // 0-4

    let strength: PasswordStrength;
    let color: string;
    let percentage: number;

    // Map zxcvbn score (0-4) to our strength levels
    switch (score) {
        case 0:
            strength = PasswordStrength.VeryWeak;
            color = "#dc3545"; // danger red
            percentage = 10;
            break;
        case 1:
            strength = PasswordStrength.Weak;
            color = "#fd7e14"; // warning orange
            percentage = 30;
            break;
        case 2:
            strength = PasswordStrength.Fair;
            color = "#ffc107"; // warning yellow
            percentage = 50;
            break;
        case 3:
            strength = PasswordStrength.Strong;
            color = "#28a745"; // success green
            percentage = 75;
            break;
        default:
            strength = PasswordStrength.VeryStrong;
            color = "#20c997"; // teal/cyan
            percentage = 100;
            break;
    }

    return {
        strength,
        color,
        percentage,
    };
}
