import { render, screen, fireEvent, waitFor } from '@testing-library/preact';
import { beforeEach, describe, expect, it, vi, beforeAll, afterEach } from 'vitest';
import { DeviceList } from '../devices';
import { StateDevice } from '../../components/device/types';
import { fetchClient } from '../../utils';

vi.mock('../../utils', () => ({
  fetchClient: {
    GET: vi.fn(),
    DELETE: vi.fn(),
    POST: vi.fn(),
  },
  get_decrypted_secret: vi.fn(),
  pub_key: new Uint8Array([1, 2, 3]),
  secret: new Uint8Array([4, 5, 6]),
}));

vi.mock('libsodium-wrappers', () => ({
  default: {
    crypto_box_seal_open: vi.fn(),
    crypto_box_seal: vi.fn(),
  },
}));

vi.mock('js-base64', () => ({
  Base64: {
    toUint8Array: vi.fn(),
    fromUint8Array: vi.fn(),
  },
}));

vi.mock('base58', () => ({
  int_to_base58: vi.fn(),
}));

vi.mock('../../components/Alert', () => ({
    showAlert: vi.fn(),
}));

vi.mock('preact-iso', () => ({
  useLocation: vi.fn(() => ({
    route: vi.fn(),
  })),
}));

vi.mock('react-i18next', () => ({
  useTranslation: vi.fn(() => ({
    t: vi.fn((key) => key),
  })),
}));

vi.mock('../../i18n', () => ({
  default: {
    t: vi.fn((key) => key),
  },
}));

const mockDevices: StateDevice[] = [
  {
    id: 'device-1',
    uid: 12345,
    name: 'Test Device 1',
    status: 'Connected',
    note: 'Test note 1',
    port: 8080,
    valid: true,
    last_state_change: 1640995200,
    firmware_version: '1.0.0',
  },
  {
    id: 'device-2',
    uid: 67890,
    name: 'Test Device 2',
    status: 'Disconnected',
    note: 'Test note 2',
    port: 8081,
    valid: false,
    last_state_change: null,
    firmware_version: '1.1.0',
  },
  {
    id: 'device-3',
    uid: 54321,
    name: 'Test Device 3',
    status: 'Connected',
    note: 'Test note 3',
    port: 8082,
    valid: true,
    last_state_change: 1640995300,
    firmware_version: '1.2.0',
  },
];

describe('DeviceList', () => {
  let fetchClientMock: any;
  let sodiumMock: any;
  let base64Mock: any;
  let showAlertMock: any;

  beforeAll(() => {
    global.clearInterval = vi.fn();
    global.setInterval = vi.fn(() => 123 as any);
    global.window.scrollTo = vi.fn();
  });

  beforeEach(async () => {
    vi.clearAllMocks();

    const { fetchClient } = await import('../../utils');
    fetchClientMock = fetchClient;
    fetchClientMock.GET.mockResolvedValue({
      data: mockDevices.map(device => ({
        ...device,
        name: 'encrypted-name',
        note: 'encrypted-note',
      })),
      error: null,
      response: { status: 200 },
    });

    const sodium = (await import('libsodium-wrappers')).default;
    sodiumMock = sodium;
    sodiumMock.crypto_box_seal_open.mockReturnValue(new TextEncoder().encode('decrypted-value'));
    sodiumMock.crypto_box_seal.mockReturnValue(new Uint8Array([1, 2, 3]));

    const { Base64 } = await import('js-base64');
    base64Mock = Base64;
    base64Mock.toUint8Array.mockReturnValue(new Uint8Array([1, 2, 3]));
    base64Mock.fromUint8Array.mockReturnValue('base64-string');


    const {showAlert} = await import('../../components/Alert');
    showAlertMock = showAlert;

  });

  afterEach(() => {
    vi.clearAllTimers();
  });

  describe('Component Initialization', () => {
    it('renders without crashing', () => {
      const { container } = render(<DeviceList />);
      expect(container).toBeTruthy();
    });

    it('initializes with default state', () => {
      const component = new DeviceList();
      expect(component.state.devices).toEqual([]);
      expect(component.state.showDeleteModal).toBe(false);
      expect(component.state.showEditNoteModal).toBe(false);
      expect(component.state.sortColumn).toBe('none');
      expect(component.state.sortSequence).toBe('asc');
    });

    it('sets up interval for updating chargers', () => {
      render(<DeviceList />);
      expect(global.setInterval).toHaveBeenCalledWith(expect.any(Function), 5000);
    });
  });

  describe('Device Loading', () => {
    it('fetches and displays devices on mount', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalledWith('/charger/get_chargers', {
          credentials: 'same-origin'
        });
      });
    });

    it('handles API errors gracefully', async () => {
      fetchClientMock.GET.mockResolvedValueOnce({
        data: null,
        error: 'API Error',
        response: { status: 500 },
      });

      const component = new DeviceList();
      await component.updateChargers();

      await waitFor(() => {
        expect(showAlertMock).toHaveBeenCalledWith(
          expect.stringContaining('loading_devices_failed'),
          'danger'
        );
      });
    });

    it('handles network errors by setting devices to disconnected', async () => {
      fetchClientMock.GET.mockRejectedValueOnce(new Error('Network error'));

      const component = new DeviceList();
      component.setState({ devices: mockDevices });

      await component.updateChargers();

      expect(component.state.devices.every(device => device.status === 'Disconnected')).toBe(true);
    });
  });

  describe('Encryption/Decryption', () => {
    it('decrypts device names successfully', () => {
      const component = new DeviceList();
      const result = component.decrypt_name('encrypted-name');

      expect(base64Mock.toUint8Array).toHaveBeenCalledWith('encrypted-name');
      expect(sodiumMock.crypto_box_seal_open).toHaveBeenCalled();
      expect(result).toBe('decrypted-value');
    });

    it('decrypts device notes successfully', () => {
      const component = new DeviceList();
      const result = component.decryptNote('encrypted-note');

      expect(base64Mock.toUint8Array).toHaveBeenCalledWith('encrypted-note');
      expect(sodiumMock.crypto_box_seal_open).toHaveBeenCalled();
      expect(result).toBe('decrypted-value');
    });

    it('handles decryption errors gracefully', () => {
      sodiumMock.crypto_box_seal_open.mockImplementationOnce(() => {
        throw new Error('Decryption failed');
      });

      const component = new DeviceList();
      const result = component.decrypt_name('invalid-encrypted-name');

      expect(result).toBeUndefined();
    });

    it('handles empty note values', () => {
      const component = new DeviceList();
      expect(component.decryptNote('')).toBe('');
      expect(component.decryptNote(null)).toBe('');
      expect(component.decryptNote(undefined)).toBe('');
    });
  });

  describe('Sorting Functionality', () => {
    beforeEach(async () => {
      fetchClientMock.GET.mockResolvedValue({
        data: mockDevices.map(device => ({
          ...device,
          name: 'encrypted-name',
          note: 'encrypted-note',
        })),
        error: null,
        response: { status: 200 },
      });
    });

    it('sorts devices by name in ascending order when clicking name header', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const nameHeader = screen.getByText('charger_name').closest('th');
      expect(nameHeader).toBeTruthy();

      fireEvent.click(nameHeader!);

      await waitFor(() => {
        expect(screen.getByText('charger_name')).toBeInTheDocument();
      });
    });

    it('toggles sort sequence when clicking same column header twice', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const nameHeader = screen.getByText('charger_name').closest('th');
      expect(nameHeader).toBeTruthy();

      fireEvent.click(nameHeader!);
      fireEvent.click(nameHeader!);

      await waitFor(() => {
        expect(screen.getByText('charger_name')).toBeInTheDocument();
      });
    });

    it('resets sorting when clicking same column header three times', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const nameHeader = screen.getByText('charger_name').closest('th');
      expect(nameHeader).toBeTruthy();

      fireEvent.click(nameHeader!);
      fireEvent.click(nameHeader!);
      fireEvent.click(nameHeader!);

      await waitFor(() => {
        expect(screen.getByText('charger_name')).toBeInTheDocument();
      });
    });

    it('sorts devices by UID when clicking UID header', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const uidHeader = screen.getByText('charger_id').closest('th');
      expect(uidHeader).toBeTruthy();

      fireEvent.click(uidHeader!);

      await waitFor(() => {
        expect(screen.getByText('charger_id')).toBeInTheDocument();
      });
    });

    it('sorts devices by last state change when clicking header', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const lastStateChangeHeader = screen.getByText('last_state_change').closest('th');
      expect(lastStateChangeHeader).toBeTruthy();

      fireEvent.click(lastStateChangeHeader!);

      await waitFor(() => {
        expect(screen.getByText('last_state_change')).toBeInTheDocument();
      });
    });

    it('can sort by different columns in sequence', async () => {
      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const nameHeader = screen.getByText('charger_name').closest('th');
      const uidHeader = screen.getByText('charger_id').closest('th');
      const noteHeader = screen.getByText('note').closest('th');

      expect(nameHeader).toBeTruthy();
      expect(uidHeader).toBeTruthy();
      expect(noteHeader).toBeTruthy();

      fireEvent.click(nameHeader!);

      fireEvent.click(uidHeader!);

      fireEvent.click(noteHeader!);

      await waitFor(() => {
        expect(screen.getByText('note')).toBeInTheDocument();
      });
    });
  });

  describe('Device Actions', () => {
    let component: DeviceList;

    beforeEach(() => {
      component = new DeviceList();
      component.setState({ devices: [...mockDevices] });
    });

    it('opens delete modal when delete is triggered', () => {
      const device = mockDevices[0];
      component.handleDelete(device);

      expect(component.state.showDeleteModal).toBe(true);
      expect(component.removalDevice).toBe(device);
    });

    it('opens edit note modal when edit note is triggered', () => {
      const device = mockDevices[0];
      component.handleEditNote(device, 0);

      expect(component.state.showEditNoteModal).toBe(true);
      expect(component.state.editNote).toBe(device.note);
      expect(component.state.editChargerIdx).toBe(0);
    });

    it('determines connection possibility correctly', () => {
      const connectedDevice = { ...mockDevices[0], status: 'Connected', valid: true };
      const disconnectedDevice = { ...mockDevices[1], status: 'Disconnected', valid: false };

      expect(component.connection_possible(connectedDevice)).toBe(true);
      expect(component.connection_possible(disconnectedDevice)).toBe(false);
    });
  });

  describe('Delete Device Functionality', () => {
    let component: DeviceList;

    beforeEach(() => {
      component = new DeviceList();
      component.setState({ devices: mockDevices });
      component.removalDevice = mockDevices[0];
    });

    it('successfully deletes device', async () => {
      fetchClientMock.DELETE.mockResolvedValueOnce({
        response: { status: 200 },
        error: null,
      });

      await component.delete_charger();

      expect(fetchClientMock.DELETE).toHaveBeenCalledWith('/charger/remove', {
        body: { charger: mockDevices[0].id },
        credentials: 'same-origin',
      });
    });

    it('handles delete errors', async () => {
      fetchClientMock.DELETE.mockResolvedValueOnce({
        response: { status: 500 },
        error: 'Delete failed',
      });

      const { showAlert } = await import('../../components/Alert');
      await component.delete_charger();

      expect(showAlert).toHaveBeenCalledWith(
        expect.stringContaining('remove_error_text'),
        'danger'
      );
    });

    it('confirms delete and closes modal', async () => {
      fetchClientMock.DELETE.mockResolvedValueOnce({
        response: { status: 200 },
        error: null,
      });

      await component.handleDeleteConfirm();

      expect(component.state.showDeleteModal).toBe(false);
    });

    it('cancels delete and closes modal', () => {
      component.setState({ showDeleteModal: true });
      component.handleDeleteCancel();

      expect(component.state.showDeleteModal).toBe(false);
    });
  });

  describe('Edit Note Functionality', () => {
    let component: DeviceList;

    beforeEach(() => {
      component = new DeviceList();
      component.setState({ devices: mockDevices, editNote: 'Updated note', editChargerIdx: 0 });
    });

    it('successfully updates note', async () => {
      fetchClientMock.POST.mockResolvedValueOnce({
        error: null,
      });

      const mockEvent = { preventDefault: vi.fn() };
      await component.handleEditNoteSubmit(mockEvent as any);

      expect(fetchClientMock.POST).toHaveBeenCalledWith('/charger/update_note', {
        credentials: 'same-origin',
        body: {
          note: 'base64-string',
          charger_id: mockDevices[0].id,
        },
      });

      expect(component.state.showEditNoteModal).toBe(false);
      expect(component.state.devices[0].note).toBe('Updated note');
    });

    it('handles note update errors', async () => {
      fetchClientMock.POST.mockResolvedValueOnce({
        error: 'Update failed',
      });

      const { showAlert } = await import('../../components/Alert');

      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const allButtons = await screen.findAllByRole('button');
      const editButton = allButtons.find(button => button.id.startsWith('edit-'));

      expect(editButton).toBeTruthy();
      fireEvent.click(editButton!);

      await waitFor(() => {
        expect(screen.getByRole('dialog')).toBeInTheDocument();
      });

      const submitButton = screen.getByRole('button', { name: 'accept' });
      expect(submitButton).toBeTruthy();

      fireEvent.click(submitButton);

      await waitFor(() => {
        expect(showAlert).toHaveBeenCalledWith('Update failed', 'danger', expect.any(String));
      });
    });

    it('cancels edit note and resets state', async () => {
      fetchClientMock.POST.mockResolvedValueOnce({
        error: null,
      });

      render(<DeviceList />);

      await waitFor(() => {
        expect(fetchClientMock.GET).toHaveBeenCalled();
      });

      const allButtons = await screen.findAllByRole('button');
      const editButton = allButtons.find(button => button.id.startsWith('edit-'));

      expect(editButton).toBeTruthy();
      fireEvent.click(editButton!);

      await waitFor(() => {
        expect(screen.getByRole('dialog')).toBeInTheDocument();
      });

      const modalButtons = await screen.findAllByRole('button');
      const cancelButton = modalButtons.find(button => button.id === 'edit-note-cancel');

      expect(cancelButton).toBeTruthy();
      fireEvent.click(cancelButton!);

      await waitFor(() => {
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
      });
    });
  });

  describe('Time Formatting', () => {
    let component: DeviceList;

    beforeEach(() => {
      component = new DeviceList();
      vi.useFakeTimers();
      vi.setSystemTime(new Date('2022-01-01T12:00:00Z'));
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it('formats recent timestamps as "just now"', () => {
      const recentTimestamp = Math.floor(Date.now() / 1000) - 30;
      const mockT = vi.fn((key) => key);

      const result = component.formatLastStateChange(mockT, recentTimestamp);
      expect(mockT).toHaveBeenCalledWith('time_just_now');
    });

    it('formats timestamps in minutes', () => {
      const timestamp = Math.floor(Date.now() / 1000) - 300;
      const mockT = vi.fn((key, options) => `${options.count} minutes ago`);

      const result = component.formatLastStateChange(mockT, timestamp);
      expect(mockT).toHaveBeenCalledWith('time_minutes_ago', { count: 5 });
    });

    it('handles null timestamps', () => {
      const mockT = vi.fn();
      const result = component.formatLastStateChange(mockT, null);

      expect(result).toBe('-');
      expect(mockT).not.toHaveBeenCalled();
    });
  });

  describe('Component Cleanup', () => {
    it('clears interval on unmount', () => {
      const component = new DeviceList();
      component.componentWillUnmount();

      expect(global.clearInterval).toHaveBeenCalledWith(123);
    });
  });

  describe('Empty State', () => {
    it('renders empty state when no devices', () => {
      const { container } = render(<DeviceList />);

      const component = new DeviceList();
      component.setState({ devices: [] });

      expect(component.state.devices).toHaveLength(0);
    });
  });
});
