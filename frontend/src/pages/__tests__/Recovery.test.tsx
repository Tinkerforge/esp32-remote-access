import { render, screen, fireEvent, waitFor, cleanup, act } from '@testing-library/preact';
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from 'vitest';

import { Recovery } from '../Recovery';

// vi.mock is hoisted at the top of the file so these declarations need
// to remain at the same level; vitest will move them automatically.
vi.mock('../../components/Alert', () => ({
    showAlert: vi.fn(),
}));

vi.mock('../../components/RecoveryDataComponent', () => ({
    RecoveryDataComponent: () => null,
}));

vi.mock('preact-iso', () => ({
    useLocation: () => ({
        route: vi.fn(),
        query: { token: 'tok', email: 'e@example.com' },
    }),
}));

vi.mock('../../utils', () => ({
    AppState: { LoggedOut: 2 },
    PASSWORD_PATTERN: /.+/,
    concat_salts: vi.fn((a: Uint8Array) => a),
    fetchClient: {
        POST: vi.fn(),
        GET: vi.fn(),
        PUT: vi.fn(),
        DELETE: vi.fn(),
    },
    generate_hash: vi.fn(async () => new Uint8Array([1, 2, 3])),
    generate_random_bytes: vi.fn(() => new Uint8Array([4, 5, 6])),
    get_salt: vi.fn(async () => new Uint8Array([7, 8, 9])),
    loggedIn: { value: 0 },
}));

vi.mock('libsodium-wrappers', () => ({
    default: {
        ready: Promise.resolve(),
        crypto_box_keypair: () => ({ publicKey: new Uint8Array([1]), privateKey: new Uint8Array([2, 3, 4]) }),
        crypto_secretbox_KEYBYTES: 32,
        crypto_secretbox_NONCEBYTES: 24,
        crypto_secretbox_easy: vi.fn(() => new Uint8Array([9, 9, 9])),
    },
    crypto_box_keypair: () => ({ publicKey: new Uint8Array([1]), privateKey: new Uint8Array([2, 3, 4]) }),
    crypto_secretbox_KEYBYTES: 32,
    crypto_secretbox_NONCEBYTES: 24,
    crypto_secretbox_easy: vi.fn(() => new Uint8Array([9, 9, 9])),
}));

const alertMock = await import('../../components/Alert');
const utilsMock = await import('../../utils');
const showAlert = alertMock.showAlert as unknown as Mock;
const fetchClientPOST = utilsMock.fetchClient.POST as unknown as Mock;
const fetchClientGET = utilsMock.fetchClient.GET as unknown as Mock;

beforeEach(() => {
    vi.clearAllMocks();
    fetchClientGET.mockResolvedValue({ data: { valid: true } });
    fetchClientPOST.mockResolvedValue({ response: { status: 200 } });
});

// The Recovery page issues `/check_expiration` via `fetchClient.POST`
// rather than `GET`, so we route that one-off call through the POST mock
// and dispatch by path.
const mockExpiration = (data: unknown) => {
    fetchClientPOST.mockImplementationOnce((path: string) => {
        if (path === '/check_expiration') return Promise.resolve({ data });
        return Promise.resolve({ response: { status: 200 } });
    });
};

afterEach(() => {
    cleanup();
});

const fillPasswords = (first: string, second: string = first) => {
    const [newPassword, confirmPassword] = screen.getAllByTestId('password-input') as HTMLInputElement[];
    fireEvent.change(newPassword, { target: { value: first } });
    fireEvent.change(confirmPassword, { target: { value: second } });
};

describe('Recovery page - expiration check', () => {
    it('shows an alert and routes home when the token check returns no data', async () => {
        mockExpiration(null);
        render(<Recovery />);

        await waitFor(() => {
            expect(showAlert).toHaveBeenCalledWith('recovery.token_expired', 'danger');
        });
    });

    it('does not show the token_expired alert when the check returns data', async () => {
        mockExpiration({ valid: true });
        render(<Recovery />);

        // Let the effect's promise resolve before asserting.
        await new Promise((r) => setTimeout(r, 0));
        expect(showAlert).not.toHaveBeenCalledWith('recovery.token_expired', 'danger');
    });
});

describe('Recovery page - form validation', () => {
    it('blocks submission when the new and confirm passwords do not match', async () => {
        render(<Recovery />);

        fillPasswords('FirstPassword1!', 'DifferentPassword2!');

        const form = screen.getByTestId('form');
        fireEvent.submit(form);

        await waitFor(() => {
            expect(screen.queryByText('recovery.no_file_warning_heading')).toBeNull();
        });
        // The expiration call is allowed to complete; the recovery POST must not.
        expect(fetchClientPOST).toHaveBeenCalledTimes(1);
        expect(fetchClientPOST).toHaveBeenCalledWith('/check_expiration', expect.any(Object));
    });

    it('marks the file input as invalid when a non-JSON file is uploaded', async () => {
        render(<Recovery />);
        fillPasswords('ValidPass123!');

        const fileInput = screen.getByTestId('file-input') as HTMLInputElement;
        const file = new File(['not json'], 'recovery.txt', { type: 'text/plain' });
        // jsdom's FileList isn't directly constructible — wrap in a stub that
        // satisfies the few members the component touches.
        const fileList = {
            length: 1,
            item: (i: number) => (i === 0 ? file : null),
            0: file,
            [Symbol.iterator]: function* () { yield file; },
        } as unknown as FileList;

        Object.defineProperty(fileInput, 'files', { configurable: true, value: fileList });

        fireEvent.change(fileInput);

        await waitFor(() => {
            expect(screen.getByText('recovery.invalid_file')).toBeInTheDocument();
        });
    });
});

describe('Recovery page - file upload', () => {
    // Email/secret/hash are populated so JSON.parse succeeds, but the
    // embedded hash is wrong so the SHA-256 check fails inside the
    // component — which is the "invalid_file" branch we want to cover.
    const makeRecoveryFile = (secret: string) =>
        new File(
            [JSON.stringify({ email: 'e@example.com', secret, hash: 'wrong' })],
            'recovery.json',
            { type: 'application/json' },
        );

    it('marks the file as invalid when the SHA-256 hash does not match', async () => {
        render(<Recovery />);
        fillPasswords('ValidPass123!');

        const file = makeRecoveryFile('ignored');
        const fileList = { length: 1, item: () => file, 0: file, [Symbol.iterator]: function* () { yield file; } } as unknown as FileList;
        const fileInput = screen.getByTestId('file-input') as HTMLInputElement;
        Object.defineProperty(fileInput, 'files', { configurable: true, value: fileList });

        fireEvent.change(fileInput);

        await waitFor(() => {
            expect(screen.getByText('recovery.invalid_file')).toBeInTheDocument();
        });
    });

    it('marks the file as invalid when it is missing email/secret/hash fields', async () => {
        render(<Recovery />);
        fillPasswords('ValidPass123!');

        const file = new File([JSON.stringify({ foo: 'bar' })], 'recovery.json', { type: 'application/json' });
        const fileList = { length: 1, item: () => file, 0: file, [Symbol.iterator]: function* () { yield file; } } as unknown as FileList;
        const fileInput = screen.getByTestId('file-input') as HTMLInputElement;
        Object.defineProperty(fileInput, 'files', { configurable: true, value: fileList });

        await act(async () => {
            fireEvent.change(fileInput);
        });

        await waitFor(() => {
            expect(screen.getByText('recovery.invalid_file')).toBeInTheDocument();
        });
    });

    it('returns early when the file input has no files prop', async () => {
        render(<Recovery />);
        fillPasswords('ValidPass123!');

        const fileInput = screen.getByTestId('file-input') as HTMLInputElement;
        // null files triggers the early-return guard at the top of onChange.
        Object.defineProperty(fileInput, 'files', { configurable: true, value: null });

        // The handler returns before writing any state, so the input must
        // remain marked valid (no `invalid` class).
        await act(async () => {
            fireEvent.change(fileInput);
        });

        expect(fileInput.className).not.toContain('invalid');
    });

    it('sets state.fileValid=false when files.item(0) returns null', async () => {
        render(<Recovery />);
        fillPasswords('ValidPass123!');

        const fileInput = screen.getByTestId('file-input') as HTMLInputElement;
        // A file list with length 0 means item(0) yields null.
        const emptyFileList = {
            length: 0,
            item: () => null,
            [Symbol.iterator]: function* () {},
        } as unknown as FileList;
        Object.defineProperty(fileInput, 'files', { configurable: true, value: emptyFileList });

        await act(async () => {
            fireEvent.change(fileInput);
        });

        await waitFor(() => {
            expect(screen.getByText('recovery.invalid_file')).toBeInTheDocument();
        });
    });
});

describe('Recovery page - successful recovery submission', () => {
    it('submits the recovery request after the user confirms in the no-file modal', async () => {
        render(<Recovery />);
        fillPasswords('ValidPass123!');

        fireEvent.submit(screen.getByTestId('form'));

        const proceed = await waitFor(() =>
            screen.getByText('recovery.no_file_warning_proceed') as HTMLButtonElement,
        );
        expect(proceed.disabled).toBe(true);

        const ack = screen.getByLabelText('recovery.no_file_warning_ack');
        fireEvent.click(ack);
        expect(proceed.disabled).toBe(false);

        await act(async () => {
            fireEvent.click(proceed);
        });

        await waitFor(() => {
            expect(fetchClientPOST).toHaveBeenCalledWith('/auth/recovery', expect.any(Object));
        });
        expect(showAlert).toHaveBeenCalledWith(
            'Your new password is set!',
            'success',
            'recovery',
            'Success',
        );
    });

    it('surfaces the backend error when the recovery request fails', async () => {
        fetchClientPOST.mockImplementation((path: string) => {
            if (path === '/check_expiration') return Promise.resolve({ data: { valid: true } });
            if (path === '/auth/recovery') return Promise.resolve({ response: { status: 400 }, error: 'invalid token' });
            return Promise.resolve({ response: { status: 200 } });
        });

        render(<Recovery />);
        fillPasswords('ValidPass123!');

        fireEvent.submit(screen.getByTestId('form'));

        const proceed = await waitFor(() =>
            screen.getByText('recovery.no_file_warning_proceed') as HTMLButtonElement,
        );
        const ack = screen.getByLabelText('recovery.no_file_warning_ack');
        fireEvent.click(ack);

        await act(async () => {
            fireEvent.click(proceed);
        });

        await waitFor(() => {
            expect(showAlert).toHaveBeenCalledWith(
                expect.stringContaining('Failed to recover account with code 400'),
                'danger',
            );
        });
    });
});
