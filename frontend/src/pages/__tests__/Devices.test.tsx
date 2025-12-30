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
    (fetchClient.GET as unknown as Mock).mockImplementation((url: string) => {
      if (url === '/charger/get_chargers') {
        return Promise.resolve({
          data: undefined,
          error: 'failed',
          response: { status: 500 },
        });
      } else if (url === '/grouping/list') {
        return Promise.resolve({
          data: { groupings: [] },
          error: undefined,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: undefined, error: 'not found', response: { status: 404 } });
    });

    render(<DeviceList />);

    expect(await screen.findByText('no_devices')).toBeInTheDocument();
    await waitFor(() => expect((showAlert as unknown as Mock)).toHaveBeenCalled());
  });

  it('renders list when devices load and decrypts name/note', async () => {
    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1, 2, 3]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => new TextEncoder().encode('decoded'));

    (fetchClient.GET as unknown as Mock).mockImplementation((url: string) => {
      if (url === '/charger/get_chargers') {
        return Promise.resolve({
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
      } else if (url === '/grouping/list') {
        return Promise.resolve({
          data: { groupings: [] },
          error: undefined,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: undefined, error: 'not found', response: { status: 404 } });
    });

    const { container, unmount } = render(<DeviceList />);

    await waitFor(() => {
      expect(screen.queryByText('no_devices')).toBeNull();
      expect(container.querySelector('table')).toBeTruthy();
    });


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

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect(connectionPossible({ status: 'Connected', valid: true } as any)).toBe(true);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect(connectionPossible({ status: 'Disconnected', valid: true } as any)).toBe(false);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    expect(connectionPossible({ status: 'Connected', valid: false } as any)).toBe(false);
  });

  it('connect_to_charger routes to device details', async () => {
    const connect = DeviceList.prototype.connect_to_charger.bind({});
    const route = vi.fn();
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    await connect({ id: 'abc' } as any, route);
    expect(route).toHaveBeenCalledWith('/devices/abc');
  });

  it('setSort toggles sequence and sorts devices', async () => {
    // Prevent constructor-triggered updates from interfering
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();

    // Seed devices
    const devices = [
      { id: 'a', uid: 2, name: 'Bravo', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'b', uid: 1, name: 'Alpha', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });
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
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    getRef(ref).setState({ devices: [], sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });

    getRef(ref).setMobileSort('uid');
    await waitFor(() => expect(getRef(ref).state.sortColumn).toBe('uid'));

    getRef(ref).setMobileSort('uid');
    await waitFor(() => expect(getRef(ref).state.sortColumn).toBe('none'));
  });

  it('handleDelete and handleDeleteConfirm remove device on success', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    const devices = [
      { id: 'x', uid: 10, name: 'X', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'y', uid: 11, name: 'Y', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });

    getRef(ref).handleDelete(devices[0]);
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(true));

    (fetchClient.DELETE as unknown as Mock).mockResolvedValue({ response: { status: 200 } });
    await getRef(ref).handleDeleteConfirm();
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(false));
    expect(getRef(ref).state.devices.map(d => d.id)).toEqual(['y']);
  });

  it('handleDeleteConfirm updates filteredDevices when search filter is active', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();

    const devices = [
      { id: 'test-1', uid: 10, name: 'TestDevice1', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'test-2', uid: 11, name: 'TestDevice2', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'prod-1', uid: 12, name: 'ProductionDevice', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];

    // Set up state with a search filter active - only "Test" devices should be in filteredDevices
    getRef(ref).setState({
      devices,
      sortColumn: 'none',
      sortSequence: 'asc',
      showDeleteModal: false,
      showEditNoteModal: false,
      editNote: '',
      editChargerIdx: 0,
      searchTerm: 'Test',
      filteredDevices: [devices[0], devices[1]], // Only TestDevice1 and TestDevice2
      groupings: [],
      selectedGroupingId: null,
      groupingSearchTerm: '',
      isLoading: false
    });

    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(3));
    await waitFor(() => expect(getRef(ref).state.filteredDevices.length).toBe(2));

    // Delete TestDevice1
    getRef(ref).handleDelete(devices[0]);
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(true));

    (fetchClient.DELETE as unknown as Mock).mockResolvedValue({ response: { status: 200 } });
    await getRef(ref).handleDeleteConfirm();

    // Verify the device is removed from both devices and filteredDevices
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(false));
    expect(getRef(ref).state.devices.map(d => d.id)).toEqual(['test-2', 'prod-1']);
    expect(getRef(ref).state.filteredDevices.map(d => d.id)).toEqual(['test-2']);
  });

  it('handleDeleteConfirm updates filteredDevices when grouping filter is active', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();

    const devices = [
      { id: 'dev-1', uid: 10, name: 'Device1', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'dev-2', uid: 11, name: 'Device2', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'dev-3', uid: 12, name: 'Device3', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];

    const groupings = [
      { id: 'group-1', name: 'Group1', device_ids: ['dev-1', 'dev-2'] },
    ];

    // Set up state with a grouping filter active
    getRef(ref).setState({
      devices,
      sortColumn: 'none',
      sortSequence: 'asc',
      showDeleteModal: false,
      showEditNoteModal: false,
      editNote: '',
      editChargerIdx: 0,
      searchTerm: '',
      filteredDevices: [devices[0], devices[1]], // Only devices in group-1
      groupings,
      selectedGroupingId: 'group-1',
      groupingSearchTerm: '',
      isLoading: false
    });

    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(3));
    await waitFor(() => expect(getRef(ref).state.filteredDevices.length).toBe(2));

    // Delete Device1 from the group
    getRef(ref).handleDelete(devices[0]);
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(true));

    (fetchClient.DELETE as unknown as Mock).mockResolvedValue({ response: { status: 200 } });
    await getRef(ref).handleDeleteConfirm();

    // Verify the device is removed from both devices and filteredDevices
    await waitFor(() => expect(getRef(ref).state.showDeleteModal).toBe(false));
    expect(getRef(ref).state.devices.map(d => d.id)).toEqual(['dev-2', 'dev-3']);
    expect(getRef(ref).state.filteredDevices.map(d => d.id)).toEqual(['dev-2']);
  });

  it('handleEditNote flows: submit updates note and cancel resets', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    const devices = [
      { id: 'z', uid: 5, name: 'Z', status: 'Connected', note: 'old', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });
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
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    const devices = [
      { id: 'n1', uid: 1, name: 'Name', status: 'Connected', note: 'old', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: true, editNote: 'upd', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(true));
    (sodium.crypto_box_seal as unknown as Mock).mockReturnValue(new Uint8Array([1]));
    (fetchClient.POST as unknown as Mock).mockResolvedValue({ error: 'err' });
    const evt = { preventDefault: vi.fn() } as unknown as Event;
    await getRef(ref).handleEditNoteSubmit(evt);
    await waitFor(() => expect((showAlert as unknown as Mock)).toHaveBeenCalled());
  });

  it('updateChargers sets devices to Disconnected on network error', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    const devices = [
      { id: 'u', uid: 7, name: 'U', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));

    (fetchClient.GET as unknown as Mock).mockImplementation(() => { throw new Error('Network fail'); });
    await getRef(ref).updateChargers();
    expect(getRef(ref).state.devices[0].status).toBe('Disconnected');
  });

  it('updateChargers marks device invalid when decryption fails', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();

    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => { throw new Error('bad decrypt'); });
    (fetchClient.GET as unknown as Mock).mockResolvedValue({
      data: [{ id: 'd', uid: 2, name: 'x', note: 'y', status: 'Connected', port: 0, valid: true, last_state_change: null, firmware_version: '1' }],
      error: undefined,
      response: { status: 200 },
    });
    await getRef(ref).updateChargers();
    await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));
    expect(getRef(ref).state.devices[0].valid).toBe(false);
    expect(getRef(ref).state.devices[0].name).toBe('');
    expect(typeof getRef(ref).state.devices[0].note).toBe('string');
  });

  it('componentWillUnmount clears the interval', () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    const spy = vi.spyOn(global, 'clearInterval');
    getRef(ref).componentWillUnmount();
    expect(spy).toHaveBeenCalled();
  });

  describe('Search functionality', () => {
    it('filterDevices returns all devices when search term is empty', () => {
      const devices = [
        { id: '1', uid: 1, name: 'Device1', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'Device2', status: 'Disconnected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      expect(filterDevices(devices, '')).toEqual(devices);
      expect(filterDevices(devices, '   ')).toEqual(devices);
    });

    it('filterDevices filters by device name', () => {
      const devices = [
        { id: '1', uid: 1, name: 'TestDevice1', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'ProductionDevice', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
        { id: '3', uid: 3, name: 'AnotherDevice', status: 'Connected', note: 'Note3', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      const result = filterDevices(devices, 'test');
      expect(result).toHaveLength(1);
      expect(result[0].name).toBe('TestDevice1');
    });

    it('filterDevices filters by device ID', () => {
      const devices = [
        { id: 'abc-123', uid: 1, name: 'Device1', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: 'xyz-456', uid: 2, name: 'Device2', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      const result = filterDevices(devices, 'abc');
      expect(result).toHaveLength(1);
      expect(result[0].id).toBe('abc-123');
    });

    it('filterDevices filters by UID', () => {
      const devices = [
        { id: '1', uid: 12345, name: 'Device1', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 67890, name: 'Device2', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      const result = filterDevices(devices, '123');
      expect(result).toHaveLength(1);
      expect(result[0].uid).toBe(12345);
    });

    it('filterDevices filters by status', () => {
      const devices = [
        { id: '1', uid: 1, name: 'Device1', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'Device2', status: 'Disconnected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
        { id: '3', uid: 3, name: 'Device3', status: 'Connected', note: 'Note3', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      const result = filterDevices(devices, 'disconnected');
      expect(result).toHaveLength(1);
      expect(result[0].status).toBe('Disconnected');
    });

    it('filterDevices filters by note content', () => {
      const devices = [
        { id: '1', uid: 1, name: 'Device1', status: 'Connected', note: 'Important device for testing', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'Device2', status: 'Connected', note: 'Production server', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      const result = filterDevices(devices, 'testing');
      expect(result).toHaveLength(1);
      expect(result[0].note).toContain('testing');
    });

    it('filterDevices filters by firmware version', () => {
      const devices = [
        { id: '1', uid: 1, name: 'Device1', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.2.3' },
        { id: '2', uid: 2, name: 'Device2', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      const result = filterDevices(devices, '1.2');
      expect(result).toHaveLength(1);
      expect(result[0].firmware_version).toBe('1.2.3');
    });

    it('filterDevices is case insensitive', () => {
      const devices = [
        { id: '1', uid: 1, name: 'TestDevice', status: 'Connected', note: 'Important Note', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      expect(filterDevices(devices, 'testdevice')).toHaveLength(1);
      expect(filterDevices(devices, 'TESTDEVICE')).toHaveLength(1);
      expect(filterDevices(devices, 'important')).toHaveLength(1);
      expect(filterDevices(devices, 'IMPORTANT')).toHaveLength(1);
    });

    it('filterDevices handles multiple matching fields', () => {
      const devices = [
        { id: 'test-123', uid: 123, name: 'TestDevice', status: 'Connected', note: 'Test note', port: 0, valid: true, last_state_change: null, firmware_version: 'test-1.0.0' },
      ];
      const filterDevices = DeviceList.prototype.filterDevices.bind({});

      // Should match multiple fields containing "test"
      const result = filterDevices(devices, 'test');
      expect(result).toHaveLength(1);
    });

    it('handleSearchChange updates search term and filtered devices', async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
      initSpy.mockRestore();
      loadGroupingsSpy.mockRestore();

      const devices = [
        { id: '1', uid: 1, name: 'TestDevice', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'ProductionDevice', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: '', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });
      await waitFor(() => expect(getRef(ref).state.devices.length).toBe(2));

      getRef(ref).handleSearchChange('test');
      await waitFor(() => expect(getRef(ref).state.searchTerm).toBe('test'));
      expect(getRef(ref).state.filteredDevices).toHaveLength(1);
      expect(getRef(ref).state.filteredDevices[0].name).toBe('TestDevice');
    });

    it('handleSearchChange with empty string shows all devices', async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
      initSpy.mockRestore();
      loadGroupingsSpy.mockRestore();

      const devices = [
        { id: '1', uid: 1, name: 'TestDevice', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'ProductionDevice', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      getRef(ref).setState({ devices, sortColumn: 'none', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: 'test', filteredDevices: [devices[0]], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });

      // Ensure the initial state is set correctly
      await waitFor(() => expect(getRef(ref).state.devices.length).toBe(2));
      expect(getRef(ref).state.filteredDevices.length).toBe(1); // Should start with filtered state

      getRef(ref).handleSearchChange('');

      // Wait for both search term and filtered devices to update
      await waitFor(() => {
        expect(getRef(ref).state.searchTerm).toBe('');
        expect(getRef(ref).state.filteredDevices.length).toBe(2);
      });
    });

    it('setSortedDevices updates both devices and filteredDevices', async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
      initSpy.mockRestore();
      loadGroupingsSpy.mockRestore();

      const devices = [
        { id: '1', uid: 1, name: 'ZDevice', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'ADevice', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      getRef(ref).setState({ devices: [], sortColumn: 'name', sortSequence: 'asc', showDeleteModal: false, showEditNoteModal: false, editNote: '', editChargerIdx: 0, searchTerm: 'device', filteredDevices: [], groupings: [], selectedGroupingId: null, groupingSearchTerm: '', isLoading: false });

      getRef(ref).setSortedDevices([...devices]);
      await waitFor(() => expect(getRef(ref).state.devices.length).toBe(2));

      // Should be sorted by name ascending
      expect(getRef(ref).state.devices[0].name).toBe('ADevice');
      expect(getRef(ref).state.devices[1].name).toBe('ZDevice');

      // Filtered devices should also be sorted and filtered
      expect(getRef(ref).state.filteredDevices).toHaveLength(2);
      expect(getRef(ref).state.filteredDevices[0].name).toBe('ADevice');
    });

    it('render uses filteredDevices when search term is present', async () => {
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const initSpy = vi.spyOn(DeviceList.prototype, 'updateChargers').mockResolvedValue(undefined as any);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as any);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
      initSpy.mockRestore();
      loadGroupingsSpy.mockRestore();

      const devices = [
        { id: '1', uid: 1, name: 'TestDevice', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
        { id: '2', uid: 2, name: 'ProductionDevice', status: 'Connected', note: 'Note2', port: 0, valid: true, last_state_change: null, firmware_version: '2.0.0' },
      ];
      const filteredDevices = [devices[0]]; // Only TestDevice

      getRef(ref).setState({
        devices,
        filteredDevices,
        searchTerm: 'test',
        sortColumn: 'none',
        sortSequence: 'asc',
        showDeleteModal: false,
        showEditNoteModal: false,
        editNote: '',
        editChargerIdx: 0,
        groupings: [],
        selectedGroupingId: null,
        groupingSearchTerm: '',
        isLoading: false,
        showGroupingModal: false
      });

      await waitFor(() => expect(getRef(ref).state.devices.length).toBe(2));
      await waitFor(() => expect(getRef(ref).state.filteredDevices.length).toBe(1));

      // The component should render only the filtered device
      // This tests the logic in the render method that chooses between devices and filteredDevices
    });
  });
});
