import { render, screen, waitFor, cleanup, fireEvent } from '@testing-library/preact';
import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';
import { DeviceList } from '../Devices';
import { StateDevice } from '../../components/device/types';
import { createRef, type RefObject } from 'preact';
import { fetchClient, get_decrypted_secret } from '../../utils';
import { Base64 } from 'js-base64';
import sodium from 'libsodium-wrappers';
import { showAlert } from '../../components/Alert';

// Helper type for the mock WebSocket
interface MockWebSocket {
  url: string;
  onopen: ((ev: Event) => void) | null;
  onmessage: ((ev: MessageEvent) => void) | null;
  onerror: ((ev: Event) => void) | null;
  onclose: ((ev: CloseEvent) => void) | null;
  readyState: number;
  close: () => void;
  simulateMessage: (data: unknown) => void;
  simulateError: () => void;
}

// Access mock WebSocket instances from global
const getMockWebSocketInstances = (): MockWebSocket[] => {
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  return (globalThis as any).MockWebSocket?.instances || [];
};

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
    // Clear WebSocket instances
    getMockWebSocketInstances().length = 0;
    localStorage.clear();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders empty state when loading devices fails', async () => {
    (get_decrypted_secret as unknown as Mock).mockResolvedValue(undefined);
    (fetchClient.GET as unknown as Mock).mockImplementation((url: string) => {
      if (url === '/grouping/list') {
        return Promise.resolve({
          data: { groupings: [] },
          error: undefined,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: undefined, error: 'not found', response: { status: 404 } });
    });

    render(<DeviceList />);

    // Wait for WebSocket to connect and send empty array
    await waitFor(() => {
      const wsInstances = getMockWebSocketInstances();
      expect(wsInstances.length).toBeGreaterThan(0);
    });

    // Simulate WebSocket sending empty device list
    const wsInstance = getMockWebSocketInstances()[0];
    wsInstance.simulateMessage([]);

    expect(await screen.findByText('no_devices')).toBeInTheDocument();
  });

  it('renders list when devices load and decrypts name/note', async () => {
    (get_decrypted_secret as unknown as Mock).mockResolvedValue(undefined);
    (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1, 2, 3]));
    (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => new TextEncoder().encode('decoded'));

    (fetchClient.GET as unknown as Mock).mockImplementation((url: string) => {
      if (url === '/grouping/list') {
        return Promise.resolve({
          data: { groupings: [] },
          error: undefined,
          response: { status: 200 },
        });
      }
      return Promise.resolve({ data: undefined, error: 'not found', response: { status: 404 } });
    });

    const { container, unmount } = render(<DeviceList />);

    // Wait for WebSocket to connect
    await waitFor(() => {
      const wsInstances = getMockWebSocketInstances();
      expect(wsInstances.length).toBeGreaterThan(0);
    });

    // Simulate WebSocket sending device list
    const wsInstance = getMockWebSocketInstances()[0];
    wsInstance.simulateMessage([
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
    ]);

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
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
      { id: 'group-1', name: 'Group1', device_ids: ['dev-1', 'dev-2'], is_default: false },
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
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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

    getRef(ref).handleEditNote(devices[0]);
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(true));

    (sodium.crypto_box_seal as unknown as Mock).mockReturnValue(new Uint8Array([9, 9]));
    (fetchClient.POST as unknown as Mock).mockResolvedValue({ error: undefined });
    const evt = { preventDefault: vi.fn() } as unknown as Event;
    getRef(ref).setState({ editNote: 'new', editChargerIdx: 0 });
    await getRef(ref).handleEditNoteSubmit(evt);
    await waitFor(() => expect(getRef(ref).state.devices[0].note).toBe('new'));
    expect(getRef(ref).state.showEditNoteModal).toBe(false);

    getRef(ref).handleEditNote(devices[0]);
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(true));
    getRef(ref).handleEditNoteCancel();
    await waitFor(() => expect(getRef(ref).state.showEditNoteModal).toBe(false));
    expect(getRef(ref).state.editNote).toBe('');
    expect(getRef(ref).state.editChargerIdx).toBe(-1);
  });

  it('handleEditNoteSubmit shows alert on error', async () => {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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

  it('componentWillUnmount closes the WebSocket', async () => {
    // Only mock loadGroupings - let connectStateUpdateWebSocket run to create the WebSocket
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    loadGroupingsSpy.mockRestore();

    // Wait for WebSocket to be created
    await waitFor(() => {
      expect(getRef(ref).stateUpdateWs).not.toBeNull();
    });

    const ws = getRef(ref).stateUpdateWs;
    const closeSpy = vi.spyOn(ws!, 'close');

    getRef(ref).componentWillUnmount();
    expect(closeSpy).toHaveBeenCalled();
    expect(getRef(ref).stateUpdateWs).toBeNull();
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
      const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
      const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
      const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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
          const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
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

        });

        it('still renders the toolbar when a group filter matches no devices', async () => {
          // Reproduces a bug where applying a group filter that produced an
          // empty result also removed the toolbar (search input, group filter
          // dropdown, group-by toggle, manage groupings button), leaving the
          // user no way to clear the filter.
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
          const ref = createRef<DeviceList>();
          // @ts-expect-error - ref is valid but types dont allow it
          render(<DeviceList ref={ref} />);
          initSpy.mockRestore();
          loadGroupingsSpy.mockRestore();

          const devices = [
            { id: '1', uid: 1, name: 'TestDevice', status: 'Connected', note: 'Note1', port: 0, valid: true, last_state_change: null, firmware_version: '1.0.0' },
          ];
          const groupings = [
            { id: 'empty-group', name: 'Empty Group', device_ids: [] as string[], is_default: false },
          ];

          getRef(ref).setState({
            devices,
            filteredDevices: [],
            searchTerm: '',
            sortColumn: 'none',
            sortSequence: 'asc',
            showDeleteModal: false,
            showEditNoteModal: false,
            editNote: '',
            editChargerIdx: 0,
            groupings,
            selectedGroupingId: 'empty-group',
            groupingSearchTerm: '',
            isLoading: false,
            showGroupingModal: false,
          });

          await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));
          await waitFor(() => expect(getRef(ref).state.filteredDevices.length).toBe(0));

          // The "no devices found" hint is still rendered, but the toolbar
          // around it must stay visible so the user can adjust the filter.
          expect(await screen.findByText('no_devices_found')).toBeInTheDocument();
          expect(screen.getAllByPlaceholderText('search_devices_placeholder').length).toBeGreaterThan(0);
          expect(screen.getAllByLabelText('group_by_toggle').length).toBeGreaterThan(0);
          expect(screen.getAllByText('manage_groupings').length).toBeGreaterThan(0);
        });
  });

  describe('group-by toggle', () => {
    const setup = async () => {
      // Mock only `loadGroupings` so the constructor's WS call runs and
      // creates a real (mocked) WebSocket we can drive from the test.
      const loadSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
      render(<DeviceList ref={ref} />);
      loadSpy.mockRestore();
      await waitFor(() => expect(getMockWebSocketInstances().length).toBeGreaterThan(0));
      const wsInstance = getMockWebSocketInstances()[0];
      wsInstance.simulateMessage([
        { id: 'd-1', uid: 1, name: 'b64', note: 'b64', status: 'Connected', port: 1, valid: true, last_state_change: null, firmware_version: '1' },
      ]);
      await waitFor(() => expect(getRef(ref).state.devices.length).toBe(1));
      return ref;
    };

    it('defaults to bundled view when localStorage has no preference', async () => {
      localStorage.clear();
      const ref = await setup();
      expect(getRef(ref).state.groupByEnabled).toBe(true);
    });

    it('reads the persisted preference from localStorage on mount', async () => {
      localStorage.setItem('groupByEnabled', 'false');
      const connectSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
      const loadSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
      render(<DeviceList ref={ref} />);
      connectSpy.mockRestore();
      loadSpy.mockRestore();
      expect(getRef(ref).state.groupByEnabled).toBe(false);
    });

    it('handleGroupByToggle flips the state and writes to localStorage', async () => {
      const ref = await setup();
      expect(getRef(ref).state.groupByEnabled).toBe(true);
      expect(localStorage.getItem('groupByEnabled')).toBe(null);

      getRef(ref).handleGroupByToggle();

      await waitFor(() => expect(getRef(ref).state.groupByEnabled).toBe(false));
      expect(localStorage.getItem('groupByEnabled')).toBe('false');

      getRef(ref).handleGroupByToggle();

      await waitFor(() => expect(getRef(ref).state.groupByEnabled).toBe(true));
      expect(localStorage.getItem('groupByEnabled')).toBe('true');
    });

    it('renders the toggle as a compact icon-only button with a state-driven icon', async () => {

      const ref = await setup();
      expect(getRef(ref).state.isLoading).toBe(false);
      getRef(ref).setState({
        groupings: [{ id: 'g', name: 'Test Group', device_ids: ['d-1'], is_default: false }],
      });
      const buttons = await screen.findAllByLabelText('group_by_toggle');
      expect(buttons.length).toBeGreaterThan(0);
      const button = buttons[0];
      expect(button.textContent?.trim()).toBe('');
      // The accessibility label and tooltip both come from the same
      // translation key so the screen reader and hover text agree.
      expect(button.getAttribute('title')).toBe(button.getAttribute('aria-label'));
      expect(button.querySelector('[data-testid="grid-icon"]')).not.toBeNull();
      expect(button.querySelector('[data-testid="list-icon"]')).toBeNull();
      // The button is solid-primary in both states; only the icon and
      // aria-pressed communicate the toggle mode.
      expect(button.className).toContain('btn-primary');
      expect(button.getAttribute('aria-pressed')).toBe('true');

      getRef(ref).handleGroupByToggle();
      await waitFor(() => expect(getRef(ref).state.groupByEnabled).toBe(false));
      expect(button.querySelector('[data-testid="list-icon"]')).not.toBeNull();
      expect(button.querySelector('[data-testid="grid-icon"]')).toBeNull();
      expect(button.className).toContain('btn-primary');
      expect(button.getAttribute('aria-pressed')).toBe('false');
    });
  });

  describe('Local + cloud device merge', () => {
    // `deviceMatches` is a pure helper so we can call it on a bare `this`.
    const matches = DeviceList.prototype.deviceMatches;
    const merge = DeviceList.prototype.mergeDevices.bind({ deviceMatches: matches });

    const baseCloud = (overrides: Partial<StateDevice> = {}): StateDevice => ({
      id: 'cloud-1',
      uid: 12345,
      name: 'Garage',
      status: 'Connected',
      note: 'cloud note',
      port: 80,
      valid: true,
      last_state_change: 1700000000,
      firmware_version: '2.3.1',
      ...overrides,
    });

    const baseLocal = (overrides: Partial<StateDevice> = {}): StateDevice => ({
      id: '',
      uid: 0,
      name: 'WARP-ABCD',
      status: 'Connected',
      note: 'warp.local',
      port: 80,
      valid: true,
      last_state_change: null,
      firmware_version: '2.3.1',
      host: 'warp.local',
      ...overrides,
    });

    describe('deviceMatches', () => {
      it('returns true when both uids are set and equal', () => {
        const cloud = baseCloud({ uid: 12345 });
        const local = baseLocal({ uid: 12345, host: 'warp.local' });

        expect(matches(cloud, local)).toBe(true);
      });

      it('returns false when both uids are set but differ', () => {
        const cloud = baseCloud({ uid: 12345 });
        const local = baseLocal({ uid: 67890, host: 'warp.local' });

        expect(matches(cloud, local)).toBe(false);
      });

      it('returns false when the cloud uid is 0', () => {
        // 0 is the default for an unpaired cloud device; the local uid
        // is irrelevant in that case because the early-return guard
        // treats 0 as "no uid yet".
        const cloud = baseCloud({ uid: 0 });
        const local = baseLocal({ uid: 12345, host: 'warp.local' });

        expect(matches(cloud, local)).toBe(false);
      });

      it('returns false when the local uid is 0', () => {
        // The local side starts with uid: 0 until the discovery payload
        // carries one; in that state the match must not be claimed.
        const cloud = baseCloud({ uid: 12345 });
        const local = baseLocal({ uid: 0, host: 'warp.local' });

        expect(matches(cloud, local)).toBe(false);
      });

      it('returns false when both uids are 0', () => {
        const cloud = baseCloud({ uid: 0 });
        const local = baseLocal({ uid: 0, host: 'warp.local' });

        expect(matches(cloud, local)).toBe(false);
      });

      it('ignores non-uid fields when deciding a match', () => {
        // Even if every other field is wildly different, a uid match
        // must be enough for the devices to be considered the same.
        const cloud = baseCloud({
          uid: 12345,
          id: 'cloud-1',
          name: 'Garage',
          status: 'Connected',
          port: 80,
          firmware_version: '2.3.1',
        });
        const local = baseLocal({
          uid: 12345,
          id: '',
          name: 'WARP-ABCD',
          status: 'Disconnected',
          port: 8080,
          firmware_version: '9.9.9',
          host: 'new.local',
        });

        expect(matches(cloud, local)).toBe(true);
      });

      it('is symmetric for two uids that are equal', () => {
        // The function is a pure boolean, so swapping the arguments must
        // not change the result. This is a useful invariant for the
        // merge logic that iterates over `cloud` while passing each entry
        // through `find` over `local`.
        const cloud = baseCloud({ uid: 42 });
        const local = baseLocal({ uid: 42, host: 'warp.local' });

        expect(matches(cloud, local)).toBe(matches(local, cloud));
      });
    });

    it('mergeDevices pairs by uid even when the local host has moved to a new port', () => {
      const cloud = baseCloud({ uid: 12345, port: 80, firmware_version: '2.3.1' });
      const local = baseLocal({ uid: 12345, port: 8080, firmware_version: '2.3.1', host: 'new.local' });

      const merged = merge([cloud], [local]);

      expect(merged).toHaveLength(1);
      expect(merged[0].id).toBe('cloud-1');
      // The new local `host` and `port` win over the cloud values because
      // the local entry is the freshest source of LAN-side reachability:
      // these are the values the bridge actually dials to reach the
      // charger on the local network.
      expect(merged[0].host).toBe('new.local');
      expect(merged[0].port).toBe(8080);
      expect(merged[0].uid).toBe(12345);
    });

    it('mergeDevices folds a local device into its cloud counterpart', () => {
      const cloud = baseCloud();
      const local = baseLocal({ uid: 12345 });

      const merged = merge([cloud], [local]);

      expect(merged).toHaveLength(1);
      expect(merged[0].id).toBe('cloud-1');
      expect(merged[0].name).toBe('Garage');
      expect(merged[0].host).toBe('warp.local');
      // Cloud-only fields must win over the local entry's defaults.
      expect(merged[0].uid).toBe(12345);
      expect(merged[0].status).toBe('Connected');
      expect(merged[0].note).toBe('cloud note');
    });

    it('mergeDevices keeps unmatched local devices as standalone local entries', () => {
      const cloud = baseCloud({ port: 80, firmware_version: '1.0.0' });
      const local = baseLocal({ port: 8080, host: 'other.local' });

      const merged = merge([cloud], [local]);

      expect(merged).toHaveLength(2);
      // Standalone local devices share the empty id, so look them up by host.
      const mergedLocal = merged.find(d => d.host === 'other.local');
      expect(mergedLocal).toBeDefined();
      expect(mergedLocal?.id).toBe('');
      expect(mergedLocal?.host).toBe('other.local');
    });

    it('mergeDevices keeps unmatched cloud devices as cloud-only entries', () => {
      const cloud = baseCloud({ id: 'cloud-lonely', port: 80, firmware_version: '1.0.0' });
      const local = baseLocal({ port: 8080 });

      const merged = merge([cloud], [local]);

      expect(merged).toHaveLength(2);
      const cloudEntry = merged.find(d => d.id === 'cloud-lonely');
      expect(cloudEntry).toBeDefined();
      expect(cloudEntry?.host).toBeUndefined();
    });

    it('mergeDevices drops a stale host from a previous merge', () => {
      // Simulate a cloud device that was merged in a previous discovery pass
      // and has now lost its local counterpart. The stale `host` must be
      // removed so the device no longer looks reachable on the LAN.
      const previouslyMerged = baseCloud({ host: 'stale.local' });
      const noMatch = baseLocal({ port: 8080, host: 'other.local' });

      const merged = merge([previouslyMerged], [noMatch]);

      expect(merged).toHaveLength(2);
      const cloudEntry = merged.find(d => d.id === 'cloud-1');
      expect(cloudEntry?.host).toBeUndefined();
    });

    it('mergeDevices does not consume a local entry twice', () => {
      // Two cloud devices with identical match keys should not both claim the
      // same local entry. The second cloud device falls through and stays
      // cloud-only.
      const cloudA = baseCloud({ id: 'cloud-a' });
      const cloudB = baseCloud({ id: 'cloud-b' });
      const local = baseLocal({ uid: 12345 });

      const merged = merge([cloudA, cloudB], [local]);

      expect(merged).toHaveLength(2);
      const mergedA = merged.find(d => d.id === 'cloud-a');
      const mergedB = merged.find(d => d.id === 'cloud-b');
      expect(mergedA?.host).toBe('warp.local');
      expect(mergedB?.host).toBeUndefined();
    });

    it('onWarpChargersChanged merges discovered devices into the existing cloud list', async () => {
      // Set up the WARP discovery bridge BEFORE mounting so the constructor's
      // `subscribeToLocalDiscovery` actually registers the discovery callback.
      // In a real app the bridge is provided by the WARP Android app.
      const startDiscovery = vi.fn();
      const stopDiscovery = vi.fn();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).tinkerforge_discovery = {
        isSupported: () => true,
        startDiscovery,
        stopDiscovery,
        getChargers: () => '[]',
        navigateToCharger: () => {},
      };

      // Stop the other constructor side-effects so we can drive the lifecycle
      // manually.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const wsSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      const groupsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
      const ref = createRef<DeviceList>();
      // @ts-expect-error - ref is valid but types dont allow it
      render(<DeviceList ref={ref} />);
      wsSpy.mockRestore();
      groupsSpy.mockRestore();

      // The component should have started discovery via the bridge and wired
      // up the `onWarpChargersChanged` callback we are about to call.
      expect(startDiscovery).toHaveBeenCalled();
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      expect(typeof (window as any).onWarpChargersChanged).toBe('function');

      // Seed the state with a single cloud-paired device. `cloudDevices` is
      // the source of truth for the cloud side of the merge now that the
      // `local:` id prefix has been removed.
      const cloudDevice = baseCloud();
      getRef(ref).setState({
        devices: [cloudDevice],
        localDevices: [],
        cloudDevices: [cloudDevice],
        sortColumn: 'none',
        sortSequence: 'asc',
        showDeleteModal: false,
        showEditNoteModal: false,
        showGroupingModal: false,
        editNote: '',
        editChargerIdx: 0,
        searchTerm: '',
        filteredDevices: [],
        groupings: [],
        selectedGroupingId: null,
        groupingSearchTerm: '',
        isLoading: false,
      });

      // Simulate the bridge discovering the same charger on the LAN.
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).onWarpChargersChanged([{
        serviceName: 'WARP-ABCD',
        displayName: 'WARP-ABCD',
        host: 'warp.local',
        port: 80,
        brand: 'Tinkerforge',
        model: 'WARP3',
        txtvers: '1',
        uid: 12345,
        firmwareVersion: '2.3.1',
      }]);

      await waitFor(() => {
        const state = getRef(ref).state;
        // The cloud device must be marked reachable locally after the merge.
        expect(state.devices.some(d => d.id === 'cloud-1' && d.host === 'warp.local')).toBe(true);
        // And there must be exactly one device in the list, not two (the
        // local entry must have been folded into the cloud entry, not kept
        // as a separate standalone local stub).
        const localAnchorMatches = state.devices.filter(d =>
          (d.id === 'cloud-1' || d.id === '') && d.host === 'warp.local'
        );
        expect(localAnchorMatches).toHaveLength(1);
      });

            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            delete (window as any).tinkerforge_discovery;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            delete (window as any).onWarpChargersChanged;
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            delete (window as any).onWarpDiscoveryStopped;
          });
        });

        describe('state_change WebSocket envelope', () => {
          it('updates state.devices in the same tick as the state_change envelope', async () => {
            (get_decrypted_secret as unknown as Mock).mockResolvedValue(undefined);
            (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1, 2, 3]));
            (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => new TextEncoder().encode('decoded'));

            (fetchClient.GET as unknown as Mock).mockImplementation((url: string) => {
              if (url === '/grouping/list') {
                return Promise.resolve({
                  data: { groupings: [] },
                  error: undefined,
                  response: { status: 200 },
                });
              }
              return Promise.resolve({ data: undefined, error: 'not found', response: { status: 404 } });
            });

            const ref = createRef<DeviceList>();
            // @ts-expect-error - ref is valid but types dont allow it
            render(<DeviceList ref={ref} />);

            await waitFor(() => {
              const wsInstances = getMockWebSocketInstances();
              expect(wsInstances.length).toBeGreaterThan(0);
            });

            const wsInstance = getMockWebSocketInstances()[0];
            // Send the post-initial-list envelope that the backend uses for
            // ongoing updates (`{type: 'state_change', chargers: [...]}`).
            wsInstance.simulateMessage({
              type: 'state_change',
              chargers: [
                {
                  id: 'dev-state-change',
                  uid: 42,
                  name: 'b64name',
                  note: 'b64note',
                  status: 'Connected',
                  port: 1,
                  valid: true,
                  last_state_change: null,
                  firmware_version: '1.0.0',
                },
              ],
            });

            await waitFor(() => {
              expect(getRef(ref).state.devices.map(d => d.id)).toContain('dev-state-change');
              expect(getRef(ref).state.isLoading).toBe(false);
              expect(getRef(ref).state.cloudDevices.map(d => d.id)).toContain('dev-state-change');
            });
          });

          it('does not hide existing devices on a transient parse error', async () => {
            (get_decrypted_secret as unknown as Mock).mockResolvedValue(undefined);
            (Base64.toUint8Array as unknown as Mock).mockReturnValue(new Uint8Array([1, 2, 3]));
            (sodium.crypto_box_seal_open as unknown as Mock).mockImplementation(() => new TextEncoder().encode('decoded'));

            (fetchClient.GET as unknown as Mock).mockImplementation((url: string) => {
              if (url === '/grouping/list') {
                return Promise.resolve({
                  data: { groupings: [] },
                  error: undefined,
                  response: { status: 200 },
                });
              }
              return Promise.resolve({ data: undefined, error: 'not found', response: { status: 404 } });
            });

            const ref = createRef<DeviceList>();
            // @ts-expect-error - ref is valid but types dont allow it
            render(<DeviceList ref={ref} />);

            await waitFor(() => {
              const wsInstances = getMockWebSocketInstances();
              expect(wsInstances.length).toBeGreaterThan(0);
            });

            const wsInstance = getMockWebSocketInstances()[0];
            // First, a valid initial list so the page has devices to keep.
            wsInstance.simulateMessage([
              {
                id: 'dev-persistent',
                uid: 7,
                name: 'b64name',
                note: 'b64note',
                status: 'Connected',
                port: 1,
                valid: true,
                last_state_change: null,
                firmware_version: '1.0.0',
              },
            ]);

            await waitFor(() => {
              expect(getRef(ref).state.devices.map(d => d.id)).toContain('dev-persistent');
            });

            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            (wsInstance as any).onmessage?.(new MessageEvent('message', { data: '{not valid json' }));

            await waitFor(() => {
              expect(getRef(ref).state.devices.map(d => d.id)).toContain('dev-persistent');
            });
          });
        });
      });

describe('Devices.tsx - callbacks rendered in render()', () => {
  // `getRef` is scoped to its enclosing describe block above, so this
  // duplicate is needed for this nested block.
  const getRef = (ref: RefObject<DeviceList>): DeviceList => {
    const current = ref.current;
    if (!current) {
      throw new Error('DeviceList ref not set');
    }
    return current;
  };

  function mountList() {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const initSpy = vi.spyOn(DeviceList.prototype, 'connectStateUpdateWebSocket').mockResolvedValue(undefined as unknown as void);
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const loadGroupingsSpy = vi.spyOn(DeviceList.prototype, 'loadGroupings').mockResolvedValue(undefined as unknown as void);
    const ref = createRef<DeviceList>();
    // @ts-expect-error - ref is valid but types dont allow it
    render(<DeviceList ref={ref} />);
    initSpy.mockRestore();
    loadGroupingsSpy.mockRestore();
    return ref;
  }

  it('opens the grouping modal when the toolbar manage-groupings control is clicked', async () => {
    const ref = mountList();
    // Both a device (to skip the empty state) and a grouping (to render
    // the toolbar's manage-groupings button) are required.
    getRef(ref).setState({
      devices: [
        { id: 'a', uid: 1, name: 'Alpha', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      ],
      groupings: [{ id: 'g1', name: 'Group', device_ids: [], is_default: false }],
      filteredDevices: [],
      showDeleteModal: false,
      showEditNoteModal: false,
      editNote: '',
      editChargerIdx: 0,
      searchTerm: '',
      selectedGroupingId: null,
      groupingSearchTerm: '',
      isLoading: false,
    });

    await waitFor(() => {
      expect(screen.getAllByText('manage_groupings').length).toBeGreaterThan(0);
    }, { timeout: 5000 });

    fireEvent.click(screen.getAllByText('manage_groupings')[0]);
    await waitFor(() => {
      expect(getRef(ref).state.showGroupingModal).toBe(true);
    });
  });

  it('re-sorts devices in place when the mobile sort-sequence dropdown is used', async () => {
    const ref = mountList();
    const devices = [
      { id: 'a', uid: 1, name: 'Alpha', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'b', uid: 2, name: 'Bravo', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
      { id: 'c', uid: 3, name: 'Charlie', status: 'Connected', note: '', port: 0, valid: true, last_state_change: null, firmware_version: '1' },
    ];
    getRef(ref).setState({
      devices,
      sortColumn: 'name',
      sortSequence: 'asc',
      showDeleteModal: false,
      showEditNoteModal: false,
      editNote: '',
      editChargerIdx: 0,
      searchTerm: '',
      filteredDevices: [],
      groupings: [],
      selectedGroupingId: null,
      groupingSearchTerm: '',
      isLoading: false,
    });
    await waitFor(() => expect(getRef(ref).state.devices.map((d) => d.name)).toEqual(['Alpha', 'Bravo', 'Charlie']));

    // The second DropdownButton in the mobile view toggles the sort
    // sequence. Click "desc" to flip the order and verify the rendered
    // device cards re-order, since that's what the user actually sees.
    const descItem = screen.getAllByText('sorting_sequence_desc')[0];
    fireEvent.click(descItem);

    await waitFor(() => {
      expect(getRef(ref).state.sortSequence).toBe('desc');
      expect(getRef(ref).state.devices.map((d) => d.name)).toEqual(['Charlie', 'Bravo', 'Alpha']);
    });
  });
});
