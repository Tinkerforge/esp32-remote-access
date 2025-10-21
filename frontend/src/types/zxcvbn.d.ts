declare module 'zxcvbn' {
  /**
   * Represents a match pattern found in the password
   */
  interface Match {
    pattern: string;
    token: string;
    i: number;
    j: number;
    guesses: number;
    guesses_log10: number;
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    [key: string]: any;
  }

  /**
   * Feedback provided for password improvement
   */
  interface Feedback {
    warning: string;
    suggestions: string[];
  }

  /**
   * Result object returned by zxcvbn password strength estimation
   */
  interface ZxcvbnResult {
    /**
     * Password strength score from 0 (weakest) to 4 (strongest)
     */
    score: 0 | 1 | 2 | 3 | 4;

    /**
     * Estimated number of guesses needed to crack the password
     */
    guesses: number;

    /**
     * Base-10 logarithm of guesses
     */
    guesses_log10: number;

    /**
     * List of patterns matched in the password
     */
    sequence: Match[];

    /**
     * Time in milliseconds to calculate the result
     */
    calc_time: number;

    /**
     * Feedback for improving password strength
     */
    feedback: Feedback;

    /**
     * Estimated time to crack with 100 attempts per hour
     */
    crack_times_seconds: {
      online_throttling_100_per_hour: number;
      online_no_throttling_10_per_second: number;
      offline_slow_hashing_1e4_per_second: number;
      offline_fast_hashing_1e10_per_second: number;
    };

    /**
     * Human-readable crack time displays
     */
    crack_times_display: {
      online_throttling_100_per_hour: string;
      online_no_throttling_10_per_second: string;
      offline_slow_hashing_1e4_per_second: string;
      offline_fast_hashing_1e10_per_second: string;
    };
  }

  /**
   * Estimates the strength of a password
   *
   * @param password - The password to analyze
   * @param user_inputs - Optional array of user-specific data (email, username, etc.)
   *                      that should be considered weak if used in the password
   * @returns Result object containing password strength metrics and feedback
   */
  function zxcvbn(
    password: string,
    user_inputs?: Array<string | number | boolean>
  ): ZxcvbnResult;

  export = zxcvbn;
}
