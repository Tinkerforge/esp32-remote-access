import { render, screen, waitFor, cleanup } from '@testing-library/preact';
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { DeviceList } from '../Devices';
import { createRef, type RefObject } from 'preact';
import { fetchClient, get_decrypted_secret } from '../../utils';
import { Base64 } from 'js-base64';
import sodium from 'libsodium-wrappers';
import { showAlert } from '../../components/Alert';

describe('Devices.tsx - DeviceList', () => {
  // Helper to safely access the component instance without using non-null assertions
  const getRef = (ref: RefObject<DeviceList>): DeviceList => {
    const current = ref.current;
    if (!current) {
      throw new Error('DeviceList ref not set');
    }
    return current;
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders empty state when loading devices fails', async () => {
    (get_decrypted_secret as unknown as Mock).mockResolvedValue(undefined);
    (fetchClient.GET as unknown as Mock).mockResolvedValue({
      data: undefined,
      error: 'failed',
      response: { status: 500 },
    });

    render(<DeviceList />);

    expect(await screen.findByText('no_devices')).toBeInTheDocument();
    await waitFor(() => expect((showAlert as unknown as Mock)).toHaveBeenCalled());
  });

  it('renders list when devices load and decrypts name/note', async () => {
    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1, 2, 3]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => new TextEncoder().encode('decoded'));

    (fetchClient.GET as unknown as Mock).mockResolvedValue({
      data: [
        {
          id: 'dev-1',
          uid: 123,
          name: 'b64name',
          note: 'b64note',
          status: 'Connected',
          port: 1,
          valid: true,
          last_state_change: null,
          firmware_version: '1.0.0',
        },
      ],
      error: undefined,
      response: { status: 200 },
    });

    const { container, unmount } = render(<DeviceList />);

    await waitFor(() => {
      expect(screen.queryByText('no_devices')).toBeNull();
    });

    expect(container.querySelector('table')).toBeTruthy();

    unmount();
  });

  it('decryptNote returns expected values', () => {
    const decryptNote = DeviceList.prototype.decryptNote.bind({});

    expect(decryptNote(undefined as unknown as string)).toBe('');
    expect(decryptNote(null as unknown as string)).toBe('');

    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockReturnValue(new TextEncoder().encode('hello'));
    expect(decryptNote('b64')).toBe('hello');

    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => { throw new Error('bad'); });
    expect(decryptNote('b64')).toBeUndefined();
  });

  it('decrypt_name returns expected values', () => {
    const decryptName = DeviceList.prototype.decrypt_name.bind({});

    expect(decryptName('')).toBe('');

    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([2]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockReturnValue(new TextEncoder().encode('world'));
    expect(decryptName('b64')).toBe('world');

    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => { throw new Error('bad'); });
    expect(decryptName('b64')).toBeUndefined();
  });

  it('formatLastStateChange maps timestamps to human-readable keys', () => {
    const t = (key: string) => key;
    const format = DeviceList.prototype.formatLastStateChange.bind({});

    vi.useFakeTimers();
    vi.setSystemTime(new Date('2025-01-01T12:00:00Z'));

    expect(format(t, Date.now() / 1000)).toBe('time_just_now');

    expect(format(t, (Date.now() - 10 * 60 * 1000) / 1000)).toBe('time_minutes_ago');

    expect(format(t, (Date.now() - 3 * 60 * 60 * 1000) / 1000)).toBe('time_hours_ago');

    expect(format(t, (Date.now() - 2 * 24 * 60 * 60 * 1000) / 1000)).toBe('time_days_ago');

    const older = format(t, (Date.now() - 10 * 24 * 60 * 60 * 1000) / 1000);
    expect(typeof older).toBe('string');

    vi.useRealTimers();
  });

  it('connection_possible returns false for disconnected or invalid devices', () => {
    const connectionPossible = DeviceList.prototype.connection_possible.bind({});

    expect(connectionPossible({ status: 'Connected', valid: true } as any)).toBe(true);
    expect(connectionPossible({ status: 'Disconnected', valid: true } as any)).toBe(false);
    expect(connectionPossible({ status: 'Connected', valid: false } as any)).toBe(false);
  });

  it('connect_to_charger routes to device details', async () => {
    const connect = DeviceList.prototype.connect_to_charger.bind({});
    const route = vi.fn();
    await connect({ id: 'abc' } as any, route);
    expect(route).toHaveBeenCalledWith('/devices/abc');
  });

  it('setSort toggles sequence and sorts devices', async () => {
    // Prevent constructor-triggered updates from interfering
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();

    // Seed devices
    const devices = [
      { id: 'a', uid: 2, name: 'Bravo', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'b', uid: 1, name: 'Alpha', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0 });
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(2));

    // First click -> sort by name asc
    getRef(ref).setSort('name');
    await waitFor(() => expect(getRef(ref).state.sortColumn).toBe('name'));
    expect(getRef(ref).state.sortSequence).toBe('asc');
    expect(getRef(ref).state.devices.map(d => d.name)).toEqual(['Alpha', 'Bravo']);

    // Second click -> name desc
    getRef(ref).setSort('name');
    await waitFor(() => expect(getRef(ref).state.sortSequence).toBe('desc'));
    expect(getRef(ref).state.devices.map(d => d.name)).toEqual(['Bravo', 'Alpha']);

    // Third click -> none (defaults to name asc)
    getRef(ref).setSort('name');
    await waitFor(() => expect(getRef(ref).state.sortColumn).toBe('none'));
    expect(getRef(ref).state.devices.map(d => d.name)).toEqual(['Alpha', 'Bravo']);
  });

  it('setMobileSort toggles between selected and none', async () => {
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    getRef(ref).setState({ devices: [], sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0 });

    getRef(ref).setMobileSort('uid');
    await waitFor(() => expect(getRef(ref).state.sortColumn).toBe('uid'));

    getRef(ref).setMobileSort('uid');
    await waitFor(() => expect(getRef(ref).state.sortColumn).toBe('none'));
  });

  it('handleDelete and handleDeleteConfirm remove device on success', async () => {
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    const devices = [
      { id: 'x', uid: 10, name: 'X', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'y', uid: 11, name: 'Y', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0 });

    getRef(ref).handleDelete(devices[0]);
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(true));

    (fetchClient.DELETE as unknown as Mock).mockResolvedValue({ response: { status: 200 } });
    await getRef(ref).handleDeleteConfirm();
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(false));
    expect(getRef(ref).state.devices.map(d => d.id)).toEqual(['y']);
  });

  it('handleEditNote flows: submit updates note and cancel resets', async () => {
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    const devices = [
      { id: 'z', uid: 5, name: 'Z', status: 'Connected', note: 'old', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0 });
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));

    getRef(ref).handleEditNote(devices[0], 0);
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(true));

    (sodium.crypto_box_seal as unknown as Mock).mockReturnValue(new Uint8Array([9, 9]));
    (fetchClient.POST as unknown as Mock).mockResolvedValue({ error: undefined });
    const evt = { preventDefault: vi.fn() } as unknown as Event;
    getRef(ref).setState({ editNote: 'new', editChargerIdx: 0 });
    await getRef(ref).handleEditNoteSubmit(evt);
    await waitFor(() => expect(getRef(ref).state.devices[0].note).toBe('new'));
    expect(getRef(ref).state.showEditNoteModal).toBe(false);

    getRef(ref).handleEditNote(devices[0], 0);
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(true));
    getRef(ref).handleEditNoteCancel();
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(false));
    expect(getRef(ref).state.editNote).toBe('');
    expect(getRef(ref).state.editChargerIdx).toBe(-1);
  });

  it('handleEditNoteSubmit shows alert on error', async () => {
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    const devices = [
      { id: 'n1', uid: 1, name: 'Name', status: 'Connected', note: 'old', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: true, editNote: 'upd', editChargerIdx: 0 });
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(true));
    (sodium.crypto_box_seal as unknown as Mock).mockReturnValue(new Uint8Array([1]));
    (fetchClient.POST as unknown as Mock).mockResolvedValue({ error: 'err' });
    const evt = { preventDefault: vi.fn() } as unknown as Event;
    await getRef(ref).handleEditNoteSubmit(evt);
    await waitFor(() => expect((showAlert as unknown as Mock)).toHaveBeenCalled());
  });

  it('updateChargers sets devices to Disconnected on network error', async () => {
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    const devices = [
      { id: 'u', uid: 7, name: 'U', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0 });
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));

    (fetchClient.GET as unknown as Mock).mockImplementation(() => { throw new Error('Network fail'); });
    await getRef(ref).updateChargers();
    expect(getRef(ref).state.devices[0].status).toBe('Disconnected');
  });

  it('updateChargers marks device invalid when decryption fails', async () => {
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => { throw new Error('bad decrypt'); });
    (fetchClient.GET as unknown as Mock).mockResolvedValue({
      data: [{ id: 'd', uid: 2, name: 'x', note: 'y', status: 'Connected', port: 0, valid: true, last_state_change: null, firmware_version: '1' }],
      error: undefined,
      response: { status: 200 },
    });
    await getRef(ref).updateChargers();
    expect(getRef(ref).state.devices[0].valid).toBe(false);
    expect(getRef(ref).state.devices[0].name).toBe('');
    expect(typeof getRef(ref).state.devices[0].note).toBe('string');
  });

  it('componentWillUnmount clears the interval', () => {
    const ref = createRef<DeviceList>();
    render(<DeviceList ref={ref} />);
    const spy = vi.spyOn(global, 'clearInterval');
    getRef(ref).componentWillUnmount();
    expect(spy).toHaveBeenCalled();
  });
});
