import { act, cleanup, render, waitFor } from '@testing-library/preact';
import { createRef } from 'preact';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DeviceList } from '../Devices';
import { Device, Grouping, UNGROUPED_GROUP_ID } from '../../components/device/types';
import { fetchClient } from '../../utils';

interface MockSocket {
  close(): void;
  simulateMessage(data: unknown): void;
}

const sockets = () => (globalThis as unknown as {
  MockWebSocket: { instances: MockSocket[] };
}).MockWebSocket.instances;

const device = (id: string, uid: number): Device => ({
  id, uid, name: '', note: '', status: 'Connected', port: 80, valid: true, firmware_version: '1.0.0',
});

function mount() {
  const ref = createRef<DeviceList>();
  // @ts-expect-error - DeviceList's props type does not allow refs
  const view = render(<DeviceList ref={ref} />);
  if (!ref.current) throw new Error('Missing DeviceList');
  return { list: ref.current, ...view };
}

describe('device list live updates', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    sockets().length = 0;
    localStorage.clear();
    vi.mocked(fetchClient.GET).mockResolvedValue({
      data: { groupings: [] }, error: undefined, response: new Response(),
    });
  });

  afterEach(() => {
    cleanup();
    vi.restoreAllMocks();
    vi.useRealTimers();
  });

  it('applies a snapshot and rapid updates without losing changes', async () => {
    const { list } = mount();
    await act(async () => {
      const ws = sockets()[0];
      ws.simulateMessage([device('first', 1), device('second', 2)]);
      ws.simulateMessage({ type: 'state_change', charger: device('third', 3) });
      ws.simulateMessage({ type: 'state_change', charger: { ...device('second', 2), status: 'Disconnected' } });
      ws.simulateMessage({ type: 'device_removed', charger_id: 'first' });
    });
    await waitFor(() => expect(list.state.cloudDevices.map(d => d.id)).toEqual(['second', 'third']));
    expect(list.state.devices.map(d => d.id)).toEqual(['second', 'third']);
    expect(list.state.filteredDevices.map(d => d.id)).toEqual(['second', 'third']);
    expect(list.state.devices[0].status).toBe('Disconnected');
  });

  it('keeps a removed cloud device available locally and never restores its cloud entry on discovery', async () => {
    const { list } = mount();
    const local = { ...device('', 1), name: 'Local charger', note: '', host: 'warp.local' };
    await act(async () => {
      list.setState({ localDevices: [local] });
      sockets()[0].simulateMessage([device('cloud-id', 1)]);
    });
    await waitFor(() => expect(list.state.devices[0]?.host).toBe('warp.local'));
    await act(async () => {
      sockets()[0].simulateMessage({ type: 'device_removed', charger_id: 'cloud-id' });
      sockets()[0].simulateMessage({ type: 'state_change', charger: device('other', 2) });
    });
    await waitFor(() => expect(list.state.cloudDevices.map(d => d.id)).toEqual(['other']));
    expect(list.state.devices.find(d => d.host === 'warp.local')).toMatchObject({ id: '', name: 'Local charger' });
    await act(async () => { list.refreshMergedDevices(); });
    expect(list.state.devices.some(d => d.id === 'cloud-id')).toBe(false);
  });

  it('ignores late frames from a previous connection', async () => {
    vi.useFakeTimers();
    const { list } = mount();
    const old = sockets()[0];
    await act(async () => { old.close(); });
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(sockets()).toHaveLength(2);
    await act(async () => {
      sockets()[1].simulateMessage([device('current', 1)]);
      old.simulateMessage([device('stale', 2)]);
    });
    expect(list.state.devices.map(d => d.id)).toEqual(['current']);
  });

  it('cancels a pending reconnect when leaving the page', async () => {
    vi.useFakeTimers();
    const { unmount } = mount();
    await act(async () => { sockets()[0].close(); });
    unmount();
    await act(async () => { vi.advanceTimersByTime(5000); });
    expect(sockets()).toHaveLength(1);
  });

  it('refreshes grouping names, defaults and membership without replacing the current filter', async () => {
    const groups = (groupings: Grouping[]) => ({ data: { groupings }, error: undefined, response: new Response() });
    vi.mocked(fetchClient.GET).mockResolvedValue(groups([
      { id: 'selected', name: '', device_ids: ['first'], is_default: true },
    ]));
    const { list } = mount();
    await waitFor(() => expect(list.state.selectedGroupingId).toBe('selected'));
    await act(async () => { sockets()[0].simulateMessage([device('first', 1), device('second', 2)]); });
    await waitFor(() => expect(list.state.filteredDevices.map(d => d.id)).toEqual(['first']));

    vi.spyOn(list, 'decryptGroupingName').mockResolvedValue('Renamed group');
    vi.mocked(fetchClient.GET).mockResolvedValue(groups([
      { id: 'selected', name: 'encrypted', device_ids: ['second'], is_default: false },
      { id: 'new-default', name: '', device_ids: [], is_default: true },
    ]));
    await act(async () => { sockets()[0].simulateMessage({ type: 'groupings_changed' }); });
    await waitFor(() => expect(list.state.filteredDevices.map(d => d.id)).toEqual(['second']));
    expect(list.state.groupings[0].name).toBe('Renamed group');
    expect(list.state.groupings[1].is_default).toBe(true);
    expect(list.state.selectedGroupingId).toBe('selected');

    // Deleting the selected group clears its filter instead of leaving it stale.
    vi.mocked(fetchClient.GET).mockResolvedValue(groups([]));
    await act(async () => { sockets()[0].simulateMessage({ type: 'groupings_changed' }); });
    await waitFor(() => expect(list.state.selectedGroupingId).toBeNull());
    expect(list.state.filteredDevices.map(d => d.id)).toEqual(['first', 'second']);
  });

  it.each([null, UNGROUPED_GROUP_ID])('preserves the %s filter on remote grouping changes', async (selectedGroupingId) => {
    const { list } = mount();
    await waitFor(() => expect(list.applyDefaultGroupingOnLoad).toBe(false));
    await act(async () => { list.handleGroupingFilterChange(selectedGroupingId); });
    vi.mocked(fetchClient.GET).mockResolvedValue({
      data: { groupings: [{ id: 'default', name: '', device_ids: [], is_default: true }] },
      error: undefined, response: new Response(),
    });
    await act(async () => { sockets()[0].simulateMessage({ type: 'groupings_changed' }); });
    await waitFor(() => expect(list.state.groupings).toHaveLength(1));
    expect(list.state.selectedGroupingId).toBe(selectedGroupingId);
  });

  it('ignores grouping responses superseded by a newer refresh', async () => {
    const { list } = mount();
    await waitFor(() => expect(list.applyDefaultGroupingOnLoad).toBe(false));
    const response = (id: string) => ({
      data: { groupings: [{ id, name: '', device_ids: [], is_default: false }] },
      error: undefined, response: new Response(),
    });
    let resolveOld!: (value: ReturnType<typeof response>) => void;
    vi.mocked(fetchClient.GET)
      .mockImplementationOnce(() => new Promise(resolve => { resolveOld = resolve; }))
      .mockResolvedValueOnce(response('new'));
    const oldLoad = list.loadGroupings(true);
    await act(async () => { await list.loadGroupings(true); });
    expect(list.state.groupings[0].id).toBe('new');
    await act(async () => { resolveOld(response('old')); await oldLoad; });
    expect(list.state.groupings[0].id).toBe('new');
  });

  it('refreshes groupings after reconnecting so missed changes are recovered', async () => {
    vi.useFakeTimers();
    const { list } = mount();
    await act(async () => { sockets()[0].simulateMessage([]); });
    await act(async () => { sockets()[0].close(); vi.advanceTimersByTime(5000); });
    vi.mocked(fetchClient.GET).mockResolvedValue({
      data: { groupings: [{ id: 'while-offline', name: '', device_ids: [], is_default: false }] },
      error: undefined, response: new Response(),
    });
    await act(async () => { sockets()[1].simulateMessage([]); });
    await vi.waitFor(() => expect(list.state.groupings.map(g => g.id)).toEqual(['while-offline']));
  });
});
